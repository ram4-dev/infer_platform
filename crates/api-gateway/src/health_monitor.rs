use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use sqlx::Row;
use tracing::{info, warn};

use crate::cache::NodeStats;
use crate::nodes::NodeStatus;
use crate::state::AppState;

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("failed to build probe client");
        // In-memory failure counters for dev mode (no DB).
        let mut failure_counts: HashMap<String, u32> = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            probe_all(&state, &client, &mut failure_counts).await;
        }
    });
}

// ── Probe loop ────────────────────────────────────────────────────────────────

async fn probe_all(
    state: &AppState,
    client: &Client,
    failure_counts: &mut HashMap<String, u32>,
) {
    let targets = collect_targets(state).await;

    for (node_id, host, agent_port) in targets {
        let url = format!("http://{host}:{agent_port}/ping");
        let start = Instant::now();
        let (success, latency_ms) = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => (true, start.elapsed().as_millis() as i32),
            _ => (false, 0i32),
        };

        if let Some(ref pool) = state.db {
            probe_db(state, pool, &node_id, success, if success { Some(latency_ms) } else { None })
                .await;
        } else {
            probe_memory(state, failure_counts, &node_id, success).await;
        }

        if success {
            info!(node_id = node_id.as_str(), latency_ms, "health probe ok");
        } else {
            warn!(node_id = node_id.as_str(), "health probe failed");
        }
    }
}

// ── DB-mode path ──────────────────────────────────────────────────────────────

async fn probe_db(
    state: &AppState,
    pool: &sqlx::PgPool,
    node_id: &str,
    success: bool,
    latency_ms: Option<i32>,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO node_health (node_id, latency_ms, success) VALUES ($1, $2, $3)",
    )
    .bind(node_id)
    .bind(latency_ms)
    .bind(success)
    .execute(pool)
    .await
    {
        warn!(node_id, "failed to record probe: {e}");
        return;
    }

    let consec = count_consecutive_failures(pool, node_id).await;
    let status_str = if consec >= 10 {
        "offline"
    } else if consec >= 3 {
        "degraded"
    } else {
        "online"
    };

    if let Err(e) = sqlx::query("UPDATE nodes SET status = $1 WHERE id = $2")
        .bind(status_str)
        .bind(node_id)
        .execute(pool)
        .await
    {
        warn!(node_id, "failed to update node status: {e}");
    }

    if consec >= 3 {
        warn!(
            node_id,
            consec, status = status_str, "node health degraded"
        );
    }

    // Recompute and cache latency stats.
    if let Ok(stats) = compute_stats(pool, node_id).await {
        // Update in-process map (primary source).
        state
            .node_stats
            .write()
            .await
            .insert(node_id.to_string(), stats.clone());

        // Also push to Redis for cross-instance visibility.
        if let Some(ref cache) = state.latency_cache {
            if let Err(e) = cache.lock().await.set_node_stats(node_id, &stats).await {
                warn!(node_id, "failed to push latency stats to Redis: {e}");
            }
        }
    }
}

// ── Memory-mode path ──────────────────────────────────────────────────────────

async fn probe_memory(
    state: &AppState,
    failure_counts: &mut HashMap<String, u32>,
    node_id: &str,
    success: bool,
) {
    if success {
        failure_counts.remove(node_id);
    } else {
        *failure_counts.entry(node_id.to_string()).or_insert(0) += 1;
    }

    let consec = if success {
        0
    } else {
        *failure_counts.get(node_id).unwrap_or(&0)
    };

    let new_status = if consec >= 10 {
        NodeStatus::Offline
    } else if consec >= 3 {
        NodeStatus::Degraded
    } else {
        NodeStatus::Online
    };

    let mut nodes = state.nodes.write().await;
    if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
        if node.status != new_status {
            info!(
                node_id,
                old = ?node.status,
                new = ?new_status,
                "node status changed"
            );
            node.status = new_status;
        }
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn count_consecutive_failures(pool: &sqlx::PgPool, node_id: &str) -> u32 {
    let rows = match sqlx::query(
        "SELECT success FROM node_health \
         WHERE node_id = $1 ORDER BY checked_at DESC LIMIT 10",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(_) => return 0,
    };

    let mut count = 0u32;
    for row in &rows {
        let ok: bool = row.try_get("success").unwrap_or(true);
        if !ok {
            count += 1;
        } else {
            break;
        }
    }
    count
}

async fn compute_stats(pool: &sqlx::PgPool, node_id: &str) -> anyhow::Result<NodeStats> {
    let p_row = sqlx::query(
        "SELECT \
           percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms) AS p50, \
           percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95 \
         FROM node_health \
         WHERE node_id = $1 \
           AND checked_at > NOW() - INTERVAL '1 hour' \
           AND success = true",
    )
    .bind(node_id)
    .fetch_one(pool)
    .await?;

    let p50: f64 = p_row.try_get::<Option<f64>, _>("p50")?.unwrap_or(0.0);
    let p95: f64 = p_row.try_get::<Option<f64>, _>("p95")?.unwrap_or(0.0);

    let u_row = sqlx::query(
        "SELECT COUNT(*) FILTER (WHERE success)::float8 \
           / NULLIF(COUNT(*), 0)::float8 AS uptime \
         FROM node_health \
         WHERE node_id = $1 \
           AND checked_at > NOW() - INTERVAL '7 days'",
    )
    .bind(node_id)
    .fetch_one(pool)
    .await?;

    let uptime: f64 = u_row.try_get::<Option<f64>, _>("uptime")?.unwrap_or(0.0);

    Ok(NodeStats {
        p50_ms: p50,
        p95_ms: p95,
        uptime_7d: uptime,
    })
}

// ── Target collection ─────────────────────────────────────────────────────────

async fn collect_targets(state: &AppState) -> Vec<(String, String, u16)> {
    if let Some(ref pool) = state.db {
        match sqlx::query("SELECT id, host, agent_port FROM nodes")
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| {
                    let id: String = row.try_get("id").ok()?;
                    let host: String = row.try_get("host").ok()?;
                    let port: i32 = row.try_get("agent_port").ok()?;
                    Some((id, host, port as u16))
                })
                .collect(),
            Err(e) => {
                warn!("failed to fetch probe targets: {e}");
                vec![]
            }
        }
    } else {
        let nodes = state.nodes.read().await;
        nodes
            .iter()
            .map(|n| (n.id.clone(), n.host.clone(), n.agent_port))
            .collect()
    }
}
