use axum::extract::Path;

/// GET /provider → list providers
pub async fn list_providers() -> axum::Json<Vec<ProviderSummary>> {
    axum::Json(vec![])
}

/// GET /provider/:id/model → list models for a provider
pub async fn list_models(Path(_id): Path<String>) -> axum::Json<Vec<ModelSummary>> {
    axum::Json(vec![])
}

#[derive(serde::Serialize)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub available: bool,
}

#[derive(serde::Serialize)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub provider_id: String,
}
