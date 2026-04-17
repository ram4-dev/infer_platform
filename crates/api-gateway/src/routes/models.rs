use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use tracing::warn;

use crate::{
    models::{ModelListResponse, OllamaTagsResponse, OpenAIModel},
    state::AppState,
};

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/api/tags", state.ollama_url);

    let resp = reqwest::get(&url).await.map_err(|e| {
        warn!("Failed to reach Ollama at {url}: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "message": "Inference backend unavailable",
                    "type": "server_error"
                }
            })),
        )
    })?;

    let tags: OllamaTagsResponse = resp.json().await.map_err(|e| {
        warn!("Failed to parse Ollama tags: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "message": "Invalid response from inference backend",
                    "type": "server_error"
                }
            })),
        )
    })?;

    let now = Utc::now().timestamp();
    let data = tags
        .models
        .into_iter()
        .map(|m| OpenAIModel {
            id: m.name,
            object: "model",
            created: now,
            owned_by: "infer-platform".to_string(),
        })
        .collect();

    Ok(Json(ModelListResponse {
        object: "list",
        data,
    }))
}

/// GET /v1/models/{id}
///
/// Returns OpenAI-compatible model info plus platform-wide latency and uptime
/// stats aggregated across all healthy nodes.
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Json<serde_json::Value> {
    let now = Utc::now().timestamp();

    let infer_stats = build_infer_stats(&state).await;

    Json(json!({
        "id": model_id,
        "object": "model",
        "created": now,
        "owned_by": "infer-platform",
        "infer_stats": infer_stats,
    }))
}

/// Aggregate platform-wide stats:
/// - p50/p95 from the in-process node_stats map (updated by the health monitor)
/// - uptime_7d from PostgreSQL when available, else from the in-process map
async fn build_infer_stats(state: &AppState) -> serde_json::Value {
    // Collect stats from the in-process map first.
    let stats_snapshot: Vec<_> = {
        let map = state.node_stats.read().await;
        map.values().cloned().collect()
    };

    // Count healthy nodes (those with stats populated = probed at least once).
    let available_nodes = {
        let nodes = state.nodes.read().await;
        nodes
            .iter()
            .filter(|n| n.status == crate::nodes::NodeStatus::Online)
            .count()
    };

    if stats_snapshot.is_empty() && state.db.is_none() {
        return json!({
            "available_nodes": available_nodes,
            "latency_p50_ms": null,
            "latency_p95_ms": null,
            "uptime_7d": null,
        });
    }

    // Try DB aggregate for the most accurate stats.
    if let Some(ref pool) = state.db {
        if let Ok(row) = sqlx::query(
            "SELECT \
               percentile_cont(0.50) WITHIN GROUP (ORDER BY nh.latency_ms) AS p50, \
               percentile_cont(0.95) WITHIN GROUP (ORDER BY nh.latency_ms) AS p95, \
               COUNT(*) FILTER (WHERE nh.success)::float8 \
                 / NULLIF(COUNT(*), 0)::float8 AS uptime_7d \
             FROM node_health nh \
             JOIN nodes n ON nh.node_id = n.id \
             WHERE n.status = 'online' \
               AND nh.checked_at > NOW() - INTERVAL '7 days' \
               AND nh.success = true",
        )
        .fetch_one(pool)
        .await
        {
            let p50: Option<f64> = row.try_get("p50").unwrap_or(None);
            let p95: Option<f64> = row.try_get("p95").unwrap_or(None);
            let uptime: Option<f64> = row.try_get("uptime_7d").unwrap_or(None);
            return json!({
                "available_nodes": available_nodes,
                "latency_p50_ms": p50,
                "latency_p95_ms": p95,
                "uptime_7d": uptime,
            });
        }
    }

    // Fall back to aggregating the in-process map.
    if stats_snapshot.is_empty() {
        return json!({
            "available_nodes": available_nodes,
            "latency_p50_ms": null,
            "latency_p95_ms": null,
            "uptime_7d": null,
        });
    }

    let p50_avg = stats_snapshot.iter().map(|s| s.p50_ms).sum::<f64>() / stats_snapshot.len() as f64;
    let p95_avg = stats_snapshot.iter().map(|s| s.p95_ms).sum::<f64>() / stats_snapshot.len() as f64;
    let uptime_avg = stats_snapshot.iter().map(|s| s.uptime_7d).sum::<f64>() / stats_snapshot.len() as f64;

    json!({
        "available_nodes": available_nodes,
        "latency_p50_ms": p50_avg,
        "latency_p95_ms": p95_avg,
        "uptime_7d": uptime_avg,
    })
}
