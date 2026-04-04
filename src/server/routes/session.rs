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
    let sessions = store.list_all_sessions().map_err(ServerError::from)?;
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

    store.create_session(&session).map_err(ServerError::from)?;

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
        .map_err(ServerError::from)?
        .ok_or_else(|| ServerError::NotFound(format!("Session {id} not found")))?;
    Ok(Json(SessionResponse {
        id: session.id,
        title: session.title,
        time_created: session.time_created,
    }))
}

/// GET /session/{id}/messages -> list messages
///
/// Returns all messages persisted in the session in creation order.
///
/// # Errors
///
/// Returns a [`ServerError`] if the listing fails.
pub async fn list_messages(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<MessageResponse>>, ServerError> {
    let store = require_store(&state)?;
    let _session = store
        .get_session(&id)
        .map_err(ServerError::from)?
        .ok_or_else(|| ServerError::NotFound(format!("Session {id} not found")))?;
    let messages = store.list_messages(&id).map_err(ServerError::from)?;
    let dtos = messages.into_iter().map(MessageResponse::from).collect();
    Ok(Json(dtos))
}

/// GET /`session/{id}/message/{message_id`} -> get a single message
///
/// # Errors
///
/// Returns [`ServerError::NotFound`] when the message does not exist in the
/// session.
pub async fn get_message(
    Path((id, message_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<MessageResponse>, ServerError> {
    let store = require_store(&state)?;
    let message = store
        .get_message(&id, &message_id)
        .map_err(ServerError::from)?
        .ok_or_else(|| {
            ServerError::NotFound(format!("Message {message_id} not found in session {id}"))
        })?;
    Ok(Json(MessageResponse::from(message)))
}

/// POST /session/{id}/message -> send message
///
/// Delegates to [`crate::session::service::run_prompt`] and returns the full
/// assistant reply.
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

    let result = crate::session::service::run_prompt(
        &store,
        &state.event_tx,
        crate::session::service::RunOptions {
            session_id: id,
            content: req.content,
            model: req.model,
            agent: None,
            no_reply: false,
        },
    )
    .await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "assistant": result.text,
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
        .map_err(ServerError::from)?;
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

/// HTTP DTO for a single message.
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct MessageResponse {
    pub id: String,
    pub session_id: String,
    pub role: crate::session::MessageRole,
    pub parts: Vec<crate::session::Part>,
    pub time_created: i64,
    pub time_updated: i64,
}

impl From<crate::session::Message> for MessageResponse {
    fn from(m: crate::session::Message) -> Self {
        Self {
            id: m.id,
            session_id: m.session_id,
            role: m.role,
            parts: m.parts,
            time_created: m.time_created,
            time_updated: m.time_updated,
        }
    }
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
            StatusCode::NOT_FOUND,
            "expected 404 for unknown session"
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

    // -----------------------------------------------------------------------
    // GET /session/{id}/messages
    // -----------------------------------------------------------------------

    /// A freshly-created session has no messages — the endpoint must return an
    /// empty array, not a 404.
    #[tokio::test]
    async fn list_messages_returns_empty_array_for_new_session()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: an empty session in the store
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir1".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        // When: GET /session/{id}/messages
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/session/{session_id}/messages"))
                    .body(Body::empty())?,
            )
            .await?;

        // Then: 200 OK with an empty JSON array
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await?)?;
        let arr = body.as_array().ok_or("expected JSON array")?;
        assert!(arr.is_empty(), "expected empty array for new session");

        Ok(())
    }

    /// Messages pre-loaded into the store must appear in the response with the
    /// correct `role` field.
    #[tokio::test]
    async fn list_messages_returns_persisted_messages_with_correct_roles()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::session::message::{Message, MessageRole};
        use crate::session::schema::now_ms;

        // Given: a session with one user and one assistant message
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir1".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let user_msg = Message::user(session_id.clone(), "hello");
        let asst_msg = {
            let now = now_ms();
            Message {
                id: crate::session::new_id(),
                session_id: session_id.clone(),
                role: MessageRole::Assistant,
                parts: vec![crate::session::Part::text("world")],
                time_created: now,
                time_updated: now,
            }
        };
        store.add_message(&user_msg)?;
        store.add_message(&asst_msg)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        // When: GET /session/{id}/messages
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/session/{session_id}/messages"))
                    .body(Body::empty())?,
            )
            .await?;

        // Then: 200 OK with 2 messages, correct roles
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await?;
        let messages: Vec<MessageResponse> = serde_json::from_slice(&bytes)?;
        assert_eq!(messages.len(), 2, "expected exactly 2 messages");

        let roles: Vec<_> = messages.iter().map(|m| &m.role).collect();
        assert!(
            roles.contains(&&crate::session::MessageRole::User),
            "missing user message"
        );
        assert!(
            roles.contains(&&crate::session::MessageRole::Assistant),
            "missing assistant message"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // GET /session/{id}/message/{message_id}
    // -----------------------------------------------------------------------

    /// Requesting a non-existent message id must return 404.
    #[tokio::test]
    async fn get_message_returns_404_for_unknown_message_id()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a session in the store (but no messages)
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir1".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        // When: GET /session/{id}/message/no-such-id
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/session/{session_id}/message/no-such-id"))
                    .body(Body::empty())?,
            )
            .await?;

        // Then: 404 Not Found
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "expected 404 for unknown message id"
        );
        Ok(())
    }

    /// Requesting a valid message id must return that specific message with the
    /// correct id and role.
    #[tokio::test]
    async fn get_message_returns_message_by_id() -> Result<(), Box<dyn std::error::Error>> {
        use crate::session::message::Message;

        // Given: a session with one user message
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir1".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let msg = Message::user(session_id.clone(), "lookup me");
        let message_id = msg.id.clone();
        store.add_message(&msg)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        // When: GET /session/{id}/message/{message_id}
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/session/{session_id}/message/{message_id}"))
                    .body(Body::empty())?,
            )
            .await?;

        // Then: 200 OK with the correct message
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let returned: MessageResponse = serde_json::from_slice(&bytes)?;
        assert_eq!(returned.id, message_id, "returned message id must match");
        assert_eq!(
            returned.role,
            crate::session::MessageRole::User,
            "returned role must be User"
        );

        Ok(())
    }
}
