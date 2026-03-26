use axum::extract::State;

use super::super::sse::sse_handler;
use super::super::state::AppState;

/// GET /event → SSE stream
pub async fn event_stream(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    sse_handler(state.subscribe())
}
