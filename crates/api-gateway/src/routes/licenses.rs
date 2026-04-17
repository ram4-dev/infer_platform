use axum::Json;

use crate::license::APPROVED_LICENSES;

pub async fn list() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "object": "list",
        "data": APPROVED_LICENSES,
        "total": APPROVED_LICENSES.len(),
    }))
}
