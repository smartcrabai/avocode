use axum::Json;
use serde::Serialize;

/// Response body for `GET /global/health`.
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
pub struct HealthResponse {
    /// Always `true` when the server is reachable.
    pub ok: bool,
    /// Crate version string (e.g. `"0.1.1"`).
    pub version: String,
}

/// GET /global/health — liveness probe used by the SDK to wait for server
/// readiness after `avocode serve` is spawned.
pub async fn get_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::{Router, routing::get};
    use tower::ServiceExt as _;

    fn make_router() -> Router {
        Router::new().route("/global/health", get(get_health))
    }

    // -----------------------------------------------------------------------
    // GET /global/health — happy path
    // -----------------------------------------------------------------------

    /// Health endpoint must return HTTP 200.
    #[tokio::test]
    async fn get_health_returns_200_ok() -> Result<(), Box<dyn std::error::Error>> {
        // Given: health endpoint router
        let app = make_router();

        // When: GET /global/health
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/global/health")
                    .body(Body::empty())?,
            )
            .await?;

        // Then: 200 OK
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    /// Response body must contain `"ok": true` and a non-empty version string.
    #[tokio::test]
    async fn get_health_response_contains_ok_true_and_version()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: health endpoint router
        let app = make_router();

        // When: GET /global/health
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/global/health")
                    .body(Body::empty())?,
            )
            .await?;

        // Then: body deserialises to HealthResponse with ok=true and version
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024).await?;
        let body: HealthResponse = serde_json::from_slice(&bytes)?;
        assert!(body.ok, "ok field must be true");
        assert!(
            !body.version.is_empty(),
            "version must be a non-empty string"
        );
        Ok(())
    }

    /// The version string must match the crate version declared in Cargo.toml.
    #[tokio::test]
    async fn get_health_version_matches_cargo_version() -> Result<(), Box<dyn std::error::Error>> {
        // Given: health endpoint router
        let app = make_router();

        // When: GET /global/health
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/global/health")
                    .body(Body::empty())?,
            )
            .await?;

        // Then: version field equals the compile-time CARGO_PKG_VERSION
        let bytes = axum::body::to_bytes(response.into_body(), 1024).await?;
        let body: HealthResponse = serde_json::from_slice(&bytes)?;
        assert_eq!(
            body.version,
            env!("CARGO_PKG_VERSION"),
            "version must match CARGO_PKG_VERSION"
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // GET /global/health — accessible without session store
    // -----------------------------------------------------------------------

    /// The health endpoint must respond even when no session store is
    /// configured (e.g. immediately after the server starts without a DB).
    #[tokio::test]
    async fn get_health_is_accessible_from_main_router_without_store()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: main router WITHOUT a session store
        use crate::server::{AppState, create_router};
        let state = AppState::new();
        let app = create_router(state);

        // When: GET /global/health
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/global/health")
                    .body(Body::empty())?,
            )
            .await?;

        // Then: returns 200 (health must not depend on the session store)
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
