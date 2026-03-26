pub mod error;
pub mod routes;
pub mod sse;
pub mod state;

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::cors::{Any, CorsLayer};

pub use error::ServerError;
pub use state::{AppState, ServerEvent};

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route(
            "/session",
            get(routes::session::list_sessions).post(routes::session::create_session),
        )
        .route("/session/{id}", get(routes::session::get_session))
        .route("/session/{id}/message", post(routes::session::send_message))
        .route("/provider", get(routes::provider::list_providers))
        .route("/provider/{id}/model", get(routes::provider::list_models))
        .route("/config", get(routes::config::get_config))
        .route("/event", get(routes::event::event_stream))
        .route("/permission", get(routes::permission::list_pending))
        .route(
            "/permission/{id}",
            post(routes::permission::reply_permission),
        )
        .layer(cors)
        .with_state(state)
}

/// Start the HTTP server on the given port.
///
/// # Errors
///
/// Returns a [`ServerError::Internal`] if binding or serving fails.
pub async fn serve(port: u16) -> Result<(), ServerError> {
    use std::net::SocketAddr;

    let state = AppState::new();
    let app = create_router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_router_does_not_panic() {
        let state = AppState::new();
        let _router = create_router(state);
    }
}
