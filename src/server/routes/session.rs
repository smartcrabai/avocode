use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use super::super::error::ServerError;
use super::super::state::{AppState, ServerEvent};

/// GET /session → list sessions
///
/// # Errors
///
/// Returns a [`ServerError`] if the listing fails.
pub async fn list_sessions(
    State(_state): State<AppState>,
) -> Result<Json<Vec<SessionSummary>>, ServerError> {
    Ok(Json(vec![]))
}

/// POST /session → create session
///
/// # Errors
///
/// Returns a [`ServerError`] if the session cannot be created.
pub async fn create_session(
    State(_state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, ServerError> {
    Ok(Json(SessionResponse {
        id: uuid_v4_stub(),
        title: req.title,
        time_created: now_ms(),
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
/// # Errors
///
/// Returns a [`ServerError`] if the message cannot be sent.
pub async fn send_message(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let _ = state
        .event_tx
        .send(ServerEvent::SessionUpdated { session_id: id });
    Ok(Json(
        serde_json::json!({ "ok": true, "message": req.content }),
    ))
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

fn uuid_v4_stub() -> String {
    format!(
        "{:016x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )
}

fn now_ms() -> i64 {
    // Saturate at i64::MAX on overflow — timestamps beyond ~292 million years from now.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
