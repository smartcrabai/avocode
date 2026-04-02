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
        .route(
            "/session/defaults",
            get(routes::session::get_session_defaults),
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

/// Start the HTTP server on the given host and port.
///
/// # Errors
///
/// Returns a [`ServerError::Internal`] if binding or serving fails.
pub async fn serve(host: &str, port: u16) -> Result<(), ServerError> {
    let data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("avocode");
    std::fs::create_dir_all(&data_dir).map_err(|e| ServerError::Internal(e.to_string()))?;
    let store = crate::session::SessionStore::open(&data_dir.join("sessions.db"))
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    let providers = match crate::provider::models_dev::fetch_dynamic_providers().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Warning: failed to load provider catalog: {e}");
            vec![]
        }
    };
    let state = AppState::with_store_and_catalog(store, providers);
    let app = create_router(state);
    let addr: std::net::SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| ServerError::Internal(e.to_string()))?;

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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    use crate::session::schema::Session;
    use crate::session::store::SessionStore;

    #[test]
    fn create_router_does_not_panic() {
        let state = AppState::new();
        let _router = create_router(state);
    }

    #[tokio::test]
    async fn get_session_defaults_returns_200_for_known_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let mut session = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        session.config_ref = Some("my-config.toml".to_owned());
        store.create_session(&session)?;

        let state = AppState::with_store(store);
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/session/defaults?directory=%2Fworkspace%2Fproj")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
