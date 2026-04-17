use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use serde_json::json;

use crate::state::AppState;

pub async fn check(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let node_count = state.nodes.read().await.len();
    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "service": "infer-api-gateway",
            "nodes_online": node_count,
            "timestamp": Utc::now().to_rfc3339(),
        })),
    )
}
