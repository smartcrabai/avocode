use axum::extract::{Path, State};

use crate::server::state::AppState;

/// GET /provider → list providers from the catalog
pub async fn list_providers(State(state): State<AppState>) -> axum::Json<Vec<ProviderSummary>> {
    let summaries = state
        .provider_catalog
        .iter()
        .map(|p| ProviderSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            available: !p.models.is_empty(),
        })
        .collect();
    axum::Json(summaries)
}

/// GET /provider/:id/model → list models for a provider from the catalog
pub async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> axum::Json<Vec<ModelSummary>> {
    let models = state
        .provider_catalog
        .iter()
        .find(|p| p.id == id)
        .map(|p| {
            p.models
                .iter()
                .map(|m| ModelSummary {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    provider_id: m.provider_id.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    axum::Json(models)
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub available: bool,
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub provider_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ModelCapabilities, ModelCost, ModelInfo, ModelStatus, ProviderInfo};
    use crate::server::state::AppState;
    use axum::{Router, body::Body, routing::get};
    use tower::ServiceExt as _;

    fn make_provider(id: &str, model_id: &str) -> ProviderInfo {
        ProviderInfo {
            id: id.to_string(),
            name: id.to_string(),
            env: vec![],
            models: vec![ModelInfo {
                id: model_id.to_string(),
                name: model_id.to_string(),
                provider_id: id.to_string(),
                family: None,
                capabilities: ModelCapabilities::default(),
                cost: ModelCost::default(),
                context_length: None,
                output_length: None,
                status: ModelStatus::Active,
            }],
        }
    }

    fn make_router(state: AppState) -> Router {
        Router::new()
            .route("/provider", get(list_providers))
            .route("/provider/{id}/model", get(list_models))
            .with_state(state)
    }

    // ─── GET /provider ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_providers_returns_ok_with_non_empty_catalog()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: AppState pre-loaded with one provider
        let state = AppState::with_catalog(vec![make_provider("anthropic", "claude-sonnet-4-5")]);
        let app = make_router(state);

        // When: GET /provider
        let request = axum::http::Request::builder()
            .uri("/provider")
            .body(Body::empty())?;
        let response = app.oneshot(request).await?;

        // Then: HTTP 200 with at least one provider entry
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let providers: Vec<ProviderSummary> = serde_json::from_slice(&bytes)?;
        assert!(!providers.is_empty());
        assert!(providers.iter().any(|p| p.id == "anthropic"));
        Ok(())
    }

    #[tokio::test]
    async fn test_list_providers_returns_empty_for_empty_catalog()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: AppState with no connected providers
        let state = AppState::with_catalog(vec![]);
        let app = make_router(state);

        // When: GET /provider
        let request = axum::http::Request::builder()
            .uri("/provider")
            .body(Body::empty())?;
        let response = app.oneshot(request).await?;

        // Then: HTTP 200, empty array
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let providers: Vec<ProviderSummary> = serde_json::from_slice(&bytes)?;
        assert!(providers.is_empty());
        Ok(())
    }

    // ─── GET /provider/:id/model ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_models_returns_models_for_known_provider()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: AppState with openai containing gpt-4o
        let state = AppState::with_catalog(vec![make_provider("openai", "gpt-4o")]);
        let app = make_router(state);

        // When: GET /provider/openai/model
        let request = axum::http::Request::builder()
            .uri("/provider/openai/model")
            .body(Body::empty())?;
        let response = app.oneshot(request).await?;

        // Then: HTTP 200 with gpt-4o in the list
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let models: Vec<ModelSummary> = serde_json::from_slice(&bytes)?;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4o");
        assert_eq!(models[0].provider_id, "openai");
        Ok(())
    }

    #[tokio::test]
    async fn test_list_models_returns_empty_for_unknown_provider()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: catalog contains anthropic only
        let state = AppState::with_catalog(vec![make_provider("anthropic", "claude-sonnet-4-5")]);
        let app = make_router(state);

        // When: GET /provider/nonexistent/model
        let request = axum::http::Request::builder()
            .uri("/provider/nonexistent/model")
            .body(Body::empty())?;
        let response = app.oneshot(request).await?;

        // Then: HTTP 200, empty model list
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let models: Vec<ModelSummary> = serde_json::from_slice(&bytes)?;
        assert!(models.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_list_models_isolates_to_requested_provider()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: catalog with two providers, each with a different model
        let state = AppState::with_catalog(vec![
            make_provider("anthropic", "claude-sonnet-4-5"),
            make_provider("openai", "gpt-4o"),
        ]);
        let app = make_router(state);

        // When: GET /provider/anthropic/model
        let request = axum::http::Request::builder()
            .uri("/provider/anthropic/model")
            .body(Body::empty())?;
        let response = app.oneshot(request).await?;

        // Then: only anthropic's model is returned; openai's gpt-4o is absent
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000).await?;
        let models: Vec<ModelSummary> = serde_json::from_slice(&bytes)?;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "claude-sonnet-4-5");
        assert!(models.iter().all(|m| m.provider_id == "anthropic"));
        Ok(())
    }
}
