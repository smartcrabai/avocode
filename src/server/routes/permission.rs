use axum::extract::Path;

/// GET /permission → list pending permission requests
pub async fn list_pending() -> axum::Json<Vec<serde_json::Value>> {
    axum::Json(vec![])
}

/// POST /permission/:id → reply to permission request
pub async fn reply_permission(
    Path(id): Path<String>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "ok": true, "id": id, "reply": req }))
}
