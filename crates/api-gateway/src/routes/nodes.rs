use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    nodes::{NodeInfo, NodeStatus, RegisterNodeRequest},
    state::AppState,
};

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> (StatusCode, Json<NodeInfo>) {
    let node = NodeInfo {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        host: req.host,
        port: req.port,
        gpu_name: req.gpu_name,
        vram_mb: req.vram_mb,
        status: NodeStatus::Online,
        registered_at: Utc::now(),
        last_seen: Utc::now(),
    };

    let mut nodes = state.nodes.write().await;
    // Replace if node with same name already registered
    if let Some(pos) = nodes.iter().position(|n| n.name == node.name) {
        nodes[pos] = node.clone();
    } else {
        nodes.push(node.clone());
    }

    tracing::info!("Node registered: {} ({}MB VRAM)", node.name, node.vram_mb);
    (StatusCode::CREATED, Json(node))
}

pub async fn list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let nodes = state.nodes.read().await;
    Json(serde_json::json!({
        "object": "list",
        "data": *nodes,
        "total": nodes.len(),
    }))
}
