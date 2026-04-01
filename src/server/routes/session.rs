use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use super::super::error::ServerError;
use super::super::state::{AppState, ServerEvent};

fn require_session_store(
    state: &AppState,
) -> Result<&std::sync::Arc<crate::session::store::SessionStore>, ServerError> {
    state
        .session_store
        .as_ref()
        .ok_or_else(|| ServerError::Internal("session store not initialised".to_owned()))
}

/// GET /session → list sessions
///
/// Returns all sessions from the store. Each session is summarised with id and title.
///
/// # Errors
///
/// Returns a [`ServerError`] if the listing fails or the store is not initialised.
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionSummary>>, ServerError> {
    let store = require_session_store(&state)?;
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

/// POST /session → create session
///
/// Creates a new session in the store and returns its id and metadata.
///
/// # Errors
///
/// Returns a [`ServerError`] if the session cannot be created or the store is not initialised.
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, ServerError> {
    let store = require_session_store(&state)?;

    let directory = req.directory.unwrap_or_else(|| ".".to_string());
    let session = crate::session::Session::new(crate::session::new_id(), directory);
    let session_id = session.id.clone();
    let time_created = session.time_created;

    store
        .create_session(&session)
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    if let Some(ref t) = req.title {
        store
            .update_session_title(&session_id, t)
            .map_err(|e| ServerError::Internal(e.to_string()))?;
    }

    let _ = state.event_tx.send(ServerEvent::SessionCreated {
        session_id: session_id.clone(),
    });

    Ok(Json(SessionResponse {
        id: session_id,
        title: req.title,
        time_created,
    }))
}

/// GET /session/{id} → get session
///
/// # Errors
///
/// Returns [`ServerError::NotFound`] when the session does not exist.
pub async fn get_session(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<SessionResponse>, ServerError> {
    Err(ServerError::NotFound(format!("Session {id} not found")))
}

/// POST /session/{id}/message → send message
///
/// Persists the user message, invokes the shared processor for LLM streaming,
/// and returns assistant content.
///
/// # Errors
///
/// Returns a [`ServerError`] if the session store is not initialised or
/// the session is not found.
pub async fn send_message(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let store = require_session_store(&state)?.clone();

    // Verify session exists.
    store
        .get_session(&id)
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or_else(|| ServerError::NotFound(format!("Session {id} not found")))?;

    let SendMessageRequest { content, model } = req;
    let model = model.ok_or_else(|| ServerError::BadRequest("model is required".to_owned()))?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let options = crate::session::processor::ProcessOptions {
        session_id: id.clone(),
        user_message: content.clone(),
        model,
        agent: crate::session::processor::DEFAULT_AGENT.to_owned(),
        max_turns: None,
    };

    let store_clone = store.clone();
    tokio::spawn(async move {
        let _ = crate::session::processor::process(&store_clone, options, tx).await;
    });

    // Collect the full assistant text from process events.
    let mut assistant_text = String::new();
    let mut had_error = false;
    while let Some(event) = rx.recv().await {
        match &event {
            crate::session::processor::ProcessEvent::PartUpdated {
                part,
                message_id: proc_msg_id,
            } => {
                if let crate::session::Part::Text(t) = part {
                    assistant_text.clone_from(&t.text);
                }
                let _ = state.event_tx.send(ServerEvent::PartUpdated {
                    session_id: id.clone(),
                    message_id: proc_msg_id.clone(),
                    part_id: part.id().to_owned(),
                });
            }
            crate::session::processor::ProcessEvent::MessageCreated { message } => {
                let _ = state.event_tx.send(ServerEvent::MessageCreated {
                    session_id: id.clone(),
                    message_id: message.id.clone(),
                });
            }
            crate::session::processor::ProcessEvent::Error(msg) => {
                had_error = true;
                let _ = state.event_tx.send(ServerEvent::SessionUpdated {
                    session_id: id.clone(),
                });
                assistant_text = format!("Error: {msg}");
            }
            crate::session::processor::ProcessEvent::Done => break,
        }
    }

    let _ = state
        .event_tx
        .send(ServerEvent::SessionUpdated { session_id: id });

    Ok(Json(serde_json::json!({
        "ok": !had_error,
        "message": content,
        "assistant": assistant_text,
    })))
}

/// GET /session/defaults?directory=... → return the last-used `config_ref` for a directory
///
/// # Errors
///
/// Returns a [`ServerError`] if the lookup fails.
pub async fn get_session_defaults(
    State(state): State<AppState>,
    Query(query): Query<SessionDefaultsQuery>,
) -> Result<Json<SessionDefaultsResponse>, ServerError> {
    let store = require_session_store(&state)?;
    let config_ref = store
        .latest_config_for_directory(&query.directory)
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(SessionDefaultsResponse { config_ref }))
}

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
mod real_comm_tests {
    use super::*;
    use crate::server::create_router;
    use crate::session::schema::Session;
    use crate::session::store::SessionStore;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    // ─── create_session with real store ────────────────────────────────────

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

    // ─── send_message returns assistant content ────────────────────────────

    #[tokio::test]
    #[ignore = "requires enhanced send_message implementation with LLM streaming"]
    async fn send_message_returns_assistant_content() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        let body = serde_json::json!({
            "content": "Hello, respond with echo!",
            "model": "openai/gpt-4o"
        });
        let uri = format!("/session/{session_id}/message");
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        let resp_body: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await?)?;

        assert!(
            resp_body.get("assistant").is_some() || resp_body.get("content").is_some(),
            "response should contain assistant content: {resp_body}"
        );

        Ok(())
    }

    // ─── send_message broadcasts server events ─────────────────────────────

    #[tokio::test]
    #[ignore = "requires enhanced send_message implementation with event broadcasting"]
    async fn send_message_broadcasts_server_events() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let state = AppState::with_store(store);
        let mut event_rx = state.subscribe();
        let app = create_router(state);

        let body = serde_json::json!({
            "content": "Hello",
            "model": "openai/gpt-4o"
        });
        let uri = format!("/session/{session_id}/message");
        let _response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body)?))?,
            )
            .await?;

        let event = event_rx.try_recv();
        assert!(event.is_ok(), "expected a server event to be broadcast");

        Ok(())
    }

    // ─── send_message returns error for unknown session ────────────────────

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
            StatusCode::NOT_FOUND,
            "unexpected status for unknown session"
        );

        Ok(())
    }

    // ─── list_sessions returns sessions from store ─────────────────────────

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
