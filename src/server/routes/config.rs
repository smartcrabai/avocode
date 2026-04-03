use axum::extract::Query;
use serde::Deserialize;

use super::super::error::ServerError;

#[derive(Deserialize)]
pub struct ConfigQuery {
    pub directory: Option<String>,
}

/// GET /config -> get current config
///
/// Loads configuration from the given directory, or from the current working
/// directory if no `directory` query parameter is provided.
///
/// # Errors
///
/// Returns a [`ServerError`] if the config cannot be loaded.
pub async fn get_config(
    Query(query): Query<ConfigQuery>,
) -> Result<axum::Json<crate::config::Config>, ServerError> {
    let directory = match query.directory {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_dir().map_err(|e| ServerError::Internal(e.to_string()))?,
    };
    let config = tokio::task::spawn_blocking(move || crate::config::loader::load(&directory))
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(axum::Json(config))
}

#[cfg(test)]
mod tests {
    use crate::server::create_router;
    use crate::server::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    fn encode_path(path: &std::path::Path) -> String {
        path.display().to_string().replace('/', "%2F")
    }

    // --- GET /config -------------------------------------------------------------

    #[tokio::test]
    async fn test_get_config_returns_200_without_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let config: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(
            config.get("model").is_some(),
            "config should have 'model' field"
        );
        assert!(
            config.get("provider").is_some(),
            "config should have 'provider' field"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_config_returns_config_for_specified_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        // Create .git to stop the directory walk at this root
        std::fs::File::create(dir.path().join(".git"))?;
        std::fs::write(
            dir.path().join("opencode.jsonc"),
            r#"{ "model": "test-model" }"#,
        )?;
        let state = AppState::new();
        let app = create_router(state);
        let encoded = encode_path(dir.path());

        let uri = format!("/config?directory={encoded}");
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let config: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(config["model"].as_str(), Some("test-model"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_config_returns_default_for_directory_without_config()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::File::create(dir.path().join(".git"))?; // Stop directory walk
        // Point XDG_CONFIG_HOME at the empty temp dir so no global config is loaded.
        // SAFETY: no other test in this module reads XDG_CONFIG_HOME in a way that
        // would change assertions; concurrent tests either don't assert on model value
        // or use project-level configs that override the global.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path()) };
        let state = AppState::new();
        let app = create_router(state);
        let encoded = encode_path(dir.path());

        let uri = format!("/config?directory={encoded}");
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty())?)
            .await?;
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let config: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(
            config["model"].is_null(),
            "model should be null for default config"
        );
        Ok(())
    }
}
