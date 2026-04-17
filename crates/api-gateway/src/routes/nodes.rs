use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    license,
    nodes::{ModelRegistration, NodeInfo, NodeStatus, RegisterNodeRequest},
    state::AppState,
};

type RegisterResult = Result<(StatusCode, Json<NodeInfo>), (StatusCode, Json<serde_json::Value>)>;

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> RegisterResult {
    // --- License compliance check ---
    let pairs: Vec<(String, String)> = req
        .models
        .iter()
        .map(|m| (m.name.clone(), m.license.clone()))
        .collect();

    let violations = license::find_violations(&pairs);
    if !violations.is_empty() {
        let body = serde_json::json!({
            "error": "license_not_approved",
            "message": "One or more models use licenses not approved for this platform.",
            "violations": violations,
            "approved_licenses": license::APPROVED_LICENSES,
        });
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(body)));
    }

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
                if !req.models.is_empty() {
                    if let Err(e) = upsert_node_models(pool, &persisted.id, &req.models).await {
                        tracing::warn!("Failed to persist node_models for {}: {e}", persisted.id);
                    }
                }
                tracing::info!(
                    "Node upserted: {} ({}MB VRAM, {} model(s))",
                    persisted.name,
                    persisted.vram_mb,
                    req.models.len(),
                );
                return Ok((StatusCode::CREATED, Json(persisted)));
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
        "Node registered (memory): {} ({}MB VRAM, {} model(s))",
        node.name,
        node.vram_mb,
        req.models.len(),
    );
    Ok((StatusCode::CREATED, Json(node)))
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

async fn upsert_node_models(
    pool: &sqlx::PgPool,
    node_id: &str,
    models: &[ModelRegistration],
) -> anyhow::Result<()> {
    for m in models {
        sqlx::query(
            "INSERT INTO node_models (node_id, model_name, license)
             VALUES ($1, $2, $3)
             ON CONFLICT (node_id, model_name) DO UPDATE SET
                 license       = EXCLUDED.license,
                 registered_at = NOW()",
        )
        .bind(node_id)
        .bind(&m.name)
        .bind(m.license.trim().to_lowercase())
        .execute(pool)
        .await?;
    }
    Ok(())
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
