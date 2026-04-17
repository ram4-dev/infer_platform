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
        name: req.name.clone(),
        host: req.host,
        port: req.port,
        agent_port: req.agent_port,
        gpu_name: req.gpu_name,
        vram_mb: req.vram_mb,
        status: NodeStatus::Online,
        registered_at: Utc::now(),
        last_seen: Utc::now(),
    };

    if let Some(ref pool) = state.db {
        match upsert_node(pool, &node).await {
            Ok(persisted) => {
                tracing::info!(
                    "Node upserted: {} ({}MB VRAM)",
                    persisted.name,
                    persisted.vram_mb
                );
                return (StatusCode::CREATED, Json(persisted));
            }
            Err(e) => {
                tracing::error!("Failed to upsert node to DB: {e}");
            }
        }
    }

    // In-memory fallback
    let mut nodes = state.nodes.write().await;
    if let Some(pos) = nodes.iter().position(|n| n.name == node.name) {
        nodes[pos] = node.clone();
    } else {
        nodes.push(node.clone());
    }
    tracing::info!(
        "Node registered (memory): {} ({}MB VRAM)",
        node.name,
        node.vram_mb
    );
    (StatusCode::CREATED, Json(node))
}

pub async fn list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    if let Some(ref pool) = state.db {
        match list_nodes_db(pool).await {
            Ok(nodes) => {
                let total = nodes.len();
                return Json(serde_json::json!({
                    "object": "list",
                    "data": nodes,
                    "total": total,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to list nodes from DB: {e}");
            }
        }
    }

    let nodes = state.nodes.read().await;
    Json(serde_json::json!({
        "object": "list",
        "data": *nodes,
        "total": nodes.len(),
    }))
}

async fn upsert_node(pool: &sqlx::PgPool, node: &NodeInfo) -> anyhow::Result<NodeInfo> {
    // ON CONFLICT on name: update host/port/status/last_seen, preserve original id and registered_at.
    let row = sqlx::query_as::<_, DbNode>(
        "INSERT INTO nodes (id, name, host, port, agent_port, gpu_name, vram_mb, status, registered_at, last_seen)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (name) DO UPDATE SET
             host         = EXCLUDED.host,
             port         = EXCLUDED.port,
             agent_port   = EXCLUDED.agent_port,
             gpu_name     = EXCLUDED.gpu_name,
             vram_mb      = EXCLUDED.vram_mb,
             status       = 'online',
             last_seen    = EXCLUDED.last_seen
         RETURNING id, name, host, port, agent_port, gpu_name, vram_mb, status, registered_at, last_seen",
    )
    .bind(&node.id)
    .bind(&node.name)
    .bind(&node.host)
    .bind(node.port as i32)
    .bind(node.agent_port as i32)
    .bind(&node.gpu_name)
    .bind(node.vram_mb as i64)
    .bind("online")
    .bind(node.registered_at)
    .bind(node.last_seen)
    .fetch_one(pool)
    .await?;

    Ok(row.into())
}

async fn list_nodes_db(pool: &sqlx::PgPool) -> anyhow::Result<Vec<NodeInfo>> {
    let rows = sqlx::query_as::<_, DbNode>(
        "SELECT id, name, host, port, agent_port, gpu_name, vram_mb, status, registered_at, last_seen
         FROM nodes ORDER BY registered_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(sqlx::FromRow)]
struct DbNode {
    id: String,
    name: String,
    host: String,
    port: i32,
    agent_port: i32,
    gpu_name: String,
    vram_mb: i64,
    status: String,
    registered_at: chrono::DateTime<chrono::Utc>,
    last_seen: chrono::DateTime<chrono::Utc>,
}

impl From<DbNode> for NodeInfo {
    fn from(r: DbNode) -> Self {
        NodeInfo {
            id: r.id,
            name: r.name,
            host: r.host,
            port: r.port as u16,
            agent_port: r.agent_port as u16,
            gpu_name: r.gpu_name,
            vram_mb: r.vram_mb as u64,
            status: match r.status.as_str() {
                "online" => NodeStatus::Online,
                "busy" => NodeStatus::Busy,
                "degraded" => NodeStatus::Degraded,
                _ => NodeStatus::Offline,
            },
            registered_at: r.registered_at,
            last_seen: r.last_seen,
        }
    }
}
