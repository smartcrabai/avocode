/// GET /config → get current config
pub async fn get_config() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "model": null, "provider": {} }))
}
