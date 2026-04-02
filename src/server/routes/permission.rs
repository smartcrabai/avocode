use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use super::super::error::ServerError;
use super::super::state::{AppState, ServerEvent};
use crate::permission::{PermissionError, PermissionReply, PermissionRequest};

#[derive(Deserialize)]
pub struct ReplyRequest {
    pub reply: PermissionReply,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct ReplyResponse {
    pub ok: bool,
    pub id: String,
    pub reply: String,
}

/// GET /permission → list pending permission requests
pub async fn list_pending(State(state): State<AppState>) -> axum::Json<Vec<PermissionRequest>> {
    axum::Json(state.permission_manager.pending_requests())
}

/// POST /permission/:id → reply to permission request
///
/// # Errors
///
/// Returns [`ServerError::NotFound`] when the request ID does not exist.
/// Returns a [`ServerError`] for internal errors.
pub async fn reply_permission(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<ReplyRequest>,
) -> Result<axum::Json<ReplyResponse>, ServerError> {
    let reply_str = req.reply.to_string();
    state
        .permission_manager
        .reply(&id, req.reply)
        .map_err(|e| match e {
            PermissionError::NotFound(_) => {
                ServerError::NotFound(format!("permission request not found: {id}"))
            }
            PermissionError::Internal(msg) => ServerError::Internal(msg),
            PermissionError::ChannelClosed => ServerError::Internal("reply channel closed".into()),
        })?;
    let _ = state.event_tx.send(ServerEvent::PermissionReplied {
        request_id: id.clone(),
        reply: reply_str.clone(),
    });

    Ok(axum::Json(ReplyResponse {
        ok: true,
        id,
        reply: reply_str,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionManager;
    use crate::permission::schema::{
        PermissionAction, PermissionReply, PermissionRequest, PermissionRule,
    };
    use crate::server::state::AppState;
    use axum::{Router, body::Body, routing::get, routing::post};
    use std::sync::Arc;
    use tower::ServiceExt as _;

    fn make_router(state: AppState) -> Router {
        Router::new()
            .route("/permission", get(list_pending))
            .route("/permission/{id}", post(reply_permission))
            .with_state(state)
    }

    fn make_ask_request(permission: &str, pattern: &str) -> PermissionRequest {
        PermissionRequest {
            id: uuid::Uuid::now_v7().to_string(),
            session_id: "test-session".into(),
            permission: permission.into(),
            pattern: pattern.into(),
            metadata: serde_json::Value::Null,
            always_patterns: vec![],
        }
    }

    /// Creates a pending Ask request by spawning a `check()` call.
    /// Returns `(manager, request_id)` where manager is shared.
    async fn create_pending_ask(
        permission: &str,
        pattern: &str,
    ) -> (Arc<PermissionManager>, String) {
        let manager = Arc::new(PermissionManager::new());
        let ask_rules = vec![PermissionRule {
            permission: permission.into(),
            pattern: pattern.into(),
            action: PermissionAction::Ask,
        }];
        let req = make_ask_request(permission, pattern);
        let req_id = req.id.clone();
        let mgr = Arc::clone(&manager);
        tokio::spawn(async move {
            let _ = mgr.check(req, &ask_rules, &[]).await;
        });
        // Poll until the spawned task registers the pending entry (up to 500ms)
        for _ in 0..50 {
            if !manager.pending_requests().is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        (manager, req_id)
    }

    // ─── GET /permission ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_pending_returns_empty_when_no_requests()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();
        let app = make_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/permission")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let pending: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
        assert!(pending.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_list_pending_returns_pending_requests() -> Result<(), Box<dyn std::error::Error>>
    {
        let (manager, _req_id) = create_pending_ask("bash", "script.sh").await;
        let state = AppState {
            permission_manager: Arc::clone(&manager),
            ..AppState::new()
        };
        let app = make_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/permission")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let pending: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
        assert!(
            !pending.is_empty(),
            "should have at least one pending request"
        );
        Ok(())
    }

    // ─── POST /permission/{id} ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_reply_permission_returns_ok_for_valid_once_reply()
    -> Result<(), Box<dyn std::error::Error>> {
        let (manager, req_id) = create_pending_ask("bash", "*").await;
        let state = AppState {
            permission_manager: Arc::clone(&manager),
            ..AppState::new()
        };
        let app = make_router(state);

        let body = serde_json::json!({ "reply": "once" });
        let uri = format!("/permission/{req_id}");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let resp: ReplyResponse = serde_json::from_slice(&bytes)?;
        assert!(resp.ok);
        assert_eq!(resp.id, req_id);
        assert_eq!(resp.reply, "once");
        Ok(())
    }

    #[tokio::test]
    async fn test_reply_permission_returns_404_for_unknown_id()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();
        let app = make_router(state);

        let body = serde_json::json!({ "reply": "once" });
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/permission/nonexistent-id")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        Ok(())
    }

    #[tokio::test]
    async fn test_reply_permission_returns_422_for_invalid_reply()
    -> Result<(), Box<dyn std::error::Error>> {
        let (manager, req_id) = create_pending_ask("bash", "*").await;
        let state = AppState {
            permission_manager: Arc::clone(&manager),
            ..AppState::new()
        };
        let app = make_router(state);

        let body = serde_json::json!({ "reply": "invalid-value" });
        let uri = format!("/permission/{req_id}");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );

        // Clean up: reply with valid value to unblock the pending check
        manager.reply(&req_id, PermissionReply::Deny)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_reply_permission_removes_from_pending() -> Result<(), Box<dyn std::error::Error>>
    {
        let (manager, req_id) = create_pending_ask("bash", "*").await;
        let state = AppState {
            permission_manager: Arc::clone(&manager),
            ..AppState::new()
        };
        let app = make_router(state);

        let body = serde_json::json!({ "reply": "once" });
        let uri = format!("/permission/{req_id}");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert!(
            manager.pending_requests().is_empty(),
            "pending should be empty after reply"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_reply_permission_publishes_event() -> Result<(), Box<dyn std::error::Error>> {
        let (manager, req_id) = create_pending_ask("bash", "*").await;
        let state = AppState {
            permission_manager: Arc::clone(&manager),
            ..AppState::new()
        };
        let mut rx = state.subscribe();
        let app = make_router(state);

        let body = serde_json::json!({ "reply": "deny" });
        let uri = format!("/permission/{req_id}");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let event = rx
            .try_recv()
            .map_err(|e| format!("expected PermissionReplied event but got error: {e}"))?;
        match event {
            crate::server::state::ServerEvent::PermissionReplied { request_id, reply } => {
                assert_eq!(request_id, req_id);
                assert_eq!(reply, "deny");
            }
            other => {
                return Err(format!("expected PermissionReplied event, got: {other:?}").into());
            }
        }
        Ok(())
    }
}
