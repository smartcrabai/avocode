use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use super::super::error::ServerError;
use super::super::state::{AppState, ServerEvent};

/// GET /session -> list sessions
///
/// # Errors
///
/// Returns a [`ServerError`] if the listing fails.
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionSummary>>, ServerError> {
    let store = require_store(&state)?;
    // List all sessions without filtering by project (HTTP API is project-agnostic).
    let sessions = store
        .list_all_sessions()
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    let summaries = sessions
        .into_iter()
        .map(|s| SessionSummary {
            id: s.id,
            title: s.title,
        })
        .collect();
    Ok(Json(summaries))
}

/// POST /session -> create session
///
/// # Errors
///
/// Returns a [`ServerError`] if the session cannot be created.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, ServerError> {
    let store = require_store(&state)?;

    let directory = if let Some(d) = req.directory {
        d
    } else {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .map_err(|e| {
                ServerError::Internal(format!("cannot determine working directory: {e}"))
            })?
    };

    // Derive a stable project id from the directory so sessions in the same
    // project are grouped together.
    let project_id = crate::app::project_id_for_directory(&directory);

    let mut session = crate::session::Session::new(project_id, directory);
    session.title.clone_from(&req.title);
    let session_id = session.id.clone();
    let time_created = session.time_created;

    store
        .create_session(&session)
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let _ = state.event_tx.send(ServerEvent::SessionCreated {
        session_id: session_id.clone(),
    });

    Ok(Json(SessionResponse {
        id: session_id,
        title: req.title,
        time_created,
    }))
}

/// GET /session/{id} -> get session
///
/// # Errors
///
/// Returns [`ServerError::NotFound`] when the session does not exist.
pub async fn get_session(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SessionResponse>, ServerError> {
    let store = require_store(&state)?;
    let session = store
        .get_session(&id)
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound(format!("Session {id} not found")))?;
    Ok(Json(SessionResponse {
        id: session.id,
        title: session.title,
        time_created: session.time_created,
    }))
}

/// POST /session/{id}/message -> send message
///
/// Runs the processor synchronously and returns the full assistant reply.
///
/// # Errors
///
/// Returns a [`ServerError`] if the message cannot be sent.
pub async fn send_message(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let store = require_store(&state)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let options = crate::session::processor::ProcessOptions {
        session_id: id.clone(),
        user_message: req.content.clone(),
        model: req.model,
        agent: "default".to_owned(),
    };

    // Spawn the processor so the channel drain runs concurrently.
    // Without this, a long response (>64 chunks) would fill the channel and deadlock.
    let store_for_proc = store.clone();
    let proc_handle = tokio::spawn(async move {
        crate::session::processor::process(&store_for_proc, options, tx).await
    });

    let mut assistant_text = String::new();
    let mut error_message: Option<String> = None;

    while let Some(event) = rx.recv().await {
        match event {
            crate::session::processor::ProcessEvent::PartUpdated { part, .. } => {
                if let crate::session::Part::Text(t) = part {
                    assistant_text.push_str(&t.text);
                }
            }
            crate::session::processor::ProcessEvent::MessageCreated { message } => {
                let _ = state.event_tx.send(ServerEvent::MessageCreated {
                    session_id: id.clone(),
                    message_id: message.id,
                });
            }
            crate::session::processor::ProcessEvent::Done => break,
            crate::session::processor::ProcessEvent::Error(e) => {
                error_message = Some(e);
                break;
            }
        }
    }

    proc_handle
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    if let Some(err) = error_message {
        return Err(ServerError::Internal(err));
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "assistant": assistant_text,
    })))
}

/// GET /session/defaults?directory=... -> return the last-used `config_ref` for a directory
///
/// # Errors
///
/// Returns a [`ServerError`] if the lookup fails.
pub async fn get_session_defaults(
    State(state): State<AppState>,
    Query(query): Query<SessionDefaultsQuery>,
) -> Result<Json<SessionDefaultsResponse>, ServerError> {
    let store = require_store(&state)?;
    let config_ref = store
        .latest_config_for_directory(&query.directory)
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(SessionDefaultsResponse { config_ref }))
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn require_store(
    state: &AppState,
) -> Result<std::sync::Arc<crate::session::SessionStore>, ServerError> {
    state
        .session_store
        .clone()
        .ok_or_else(|| ServerError::Internal("session store not initialised".to_owned()))
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SessionDefaultsQuery {
    pub directory: String,
}

#[derive(Debug, Serialize)]
pub struct SessionDefaultsResponse {
    pub config_ref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
    pub directory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub title: Option<String>,
    pub time_created: i64,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::create_router;
    use crate::session::Session;
    use crate::session::SessionStore;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    #[tokio::test]
    async fn create_session_persists_to_store() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let store_clone = store.clone();
        let state = AppState::with_store(store);
        let app = create_router(state);

        let body = serde_json::json!({
            "title": "Test Session",
            "directory": "/tmp/test-project"
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/session")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        let resp_body: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await?)?;
        let session_id = resp_body["id"].as_str().ok_or("missing id in response")?;
        assert!(!session_id.is_empty(), "session id should not be empty");

        let sessions = store_clone.list_all_sessions()?;
        assert_eq!(sessions.len(), 1, "expected exactly one session in store");
        assert_eq!(sessions[0].id, session_id);

        Ok(())
    }

    #[tokio::test]
    async fn send_message_returns_error_for_unknown_session()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let state = AppState::with_store(store);
        let app = create_router(state);

        let body = serde_json::json!({
            "content": "Hello"
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/session/nonexistent-id/message")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "unexpected status for unknown session"
        );

        Ok(())
    }

    #[tokio::test]
    async fn list_sessions_returns_stored_sessions() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let s1 = Session::new("proj-1".to_owned(), "/dir1".to_owned());
        store.create_session(&s1)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/session").body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await?)?;
        let sessions = body.as_array().ok_or("expected array")?;
        assert!(!sessions.is_empty(), "expected at least one session");

        Ok(())
    }
}
