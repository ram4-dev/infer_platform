use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;
use serde_json::json;

use crate::state::AppState;

use axum::response::IntoResponse;

#[derive(Debug, Serialize)]
pub struct NodeProviderStats {
    pub node_id: String,
    pub node_name: String,
    pub gpu_name: String,
    pub vram_mb: i64,
    pub status: String,
    pub uptime_pct_7d: f64,
    pub avg_latency_ms_7d: Option<f64>,
    pub probe_count_7d: i64,
    pub request_count_7d: i64,
    pub tokens_in_7d: i64,
    pub tokens_out_7d: i64,
    pub tokens_served_7d: i64,
    pub estimated_earnings_usd_7d: f64,
    pub stripe_onboarding_complete: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderTotals {
    pub node_count: usize,
    pub request_count_7d: i64,
    pub tokens_served_7d: i64,
    pub estimated_earnings_usd_7d: f64,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatsResponse {
    pub nodes: Vec<NodeProviderStats>,
    pub totals: ProviderTotals,
}

/// GET /v1/internal/provider/stats
///
/// Returns per-node earnings estimate, uptime, and request analytics for the
/// last 7 days. Earnings are approximated via PROVIDER_TOKEN_RATE_USD (default
/// $0.000001/token) × PROVIDER_REVENUE_SHARE (default 0.70).
pub async fn stats(State(state): State<Arc<AppState>>) -> axum::response::Response {
    let pool = match &state.db {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": {"message": "database not configured", "type": "server_error"}
                })),
            )
                .into_response()
        }
    };

    let token_rate: f64 = std::env::var("PROVIDER_TOKEN_RATE_USD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.000_001);
    let revenue_share: f64 = std::env::var("PROVIDER_REVENUE_SHARE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.70);

    // --- 1. All nodes ---
    #[derive(sqlx::FromRow)]
    struct DbNode {
        id: String,
        name: String,
        gpu_name: String,
        vram_mb: i64,
        status: String,
    }

    let nodes = match sqlx::query_as::<_, DbNode>(
        "SELECT id, name, gpu_name, vram_mb, status FROM nodes ORDER BY registered_at DESC",
    )
    .fetch_all(pool)
    .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("provider/stats: nodes query failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": "query failed", "type": "server_error"}})),
            )
                .into_response();
        }
    };

    // --- 2. Health stats per node (last 7 days) ---
    #[derive(sqlx::FromRow)]
    struct HealthRow {
        node_id: String,
        probe_count: Option<i64>,
        success_count: Option<i64>,
        avg_latency_ms: Option<f64>,
    }

    let health_rows: Vec<HealthRow> = sqlx::query_as::<_, HealthRow>(
        "SELECT node_id,
                COUNT(*)::bigint                                  AS probe_count,
                SUM(CASE WHEN success THEN 1 ELSE 0 END)::bigint AS success_count,
                AVG(CASE WHEN success THEN latency_ms END)        AS avg_latency_ms
         FROM node_health
         WHERE checked_at > NOW() - INTERVAL '7 days'
         GROUP BY node_id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // --- 3. Usage stats per node via node_models join (last 7 days) ---
    #[derive(sqlx::FromRow)]
    struct UsageRow {
        node_id: String,
        request_count: Option<i64>,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    }

    let usage_rows: Vec<UsageRow> = sqlx::query_as::<_, UsageRow>(
        "SELECT nm.node_id,
                COUNT(*)::bigint                         AS request_count,
                COALESCE(SUM(ul.tokens_in),  0)::bigint AS tokens_in,
                COALESCE(SUM(ul.tokens_out), 0)::bigint AS tokens_out
         FROM usage_logs ul
         JOIN node_models nm ON nm.model_name = ul.model
         WHERE ul.timestamp > NOW() - INTERVAL '7 days'
         GROUP BY nm.node_id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // --- 4. Models per node ---
    #[derive(sqlx::FromRow)]
    struct ModelRow {
        node_id: String,
        model_name: String,
    }

    let model_rows: Vec<ModelRow> = sqlx::query_as::<_, ModelRow>(
        "SELECT node_id, model_name FROM node_models ORDER BY registered_at",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // --- 5. Stripe Connect onboarding status per node ---
    #[derive(sqlx::FromRow)]
    struct StripeRow {
        node_id: String,
        onboarding_complete: bool,
    }

    let stripe_rows: Vec<StripeRow> = sqlx::query_as::<_, StripeRow>(
        "SELECT node_id, onboarding_complete FROM provider_stripe_accounts",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Build lookup maps
    let health_map: HashMap<String, HealthRow> = health_rows
        .into_iter()
        .map(|r| (r.node_id.clone(), r))
        .collect();
    let usage_map: HashMap<String, UsageRow> = usage_rows
        .into_iter()
        .map(|r| (r.node_id.clone(), r))
        .collect();
    let stripe_map: HashMap<String, bool> = stripe_rows
        .into_iter()
        .map(|r| (r.node_id, r.onboarding_complete))
        .collect();
    let mut models_map: HashMap<String, Vec<String>> = HashMap::new();
    for r in model_rows {
        models_map.entry(r.node_id).or_default().push(r.model_name);
    }

    // Assemble per-node stats
    let mut node_stats: Vec<NodeProviderStats> = Vec::with_capacity(nodes.len());
    for n in nodes {
        let health = health_map.get(&n.id);
        let usage = usage_map.get(&n.id);
        let models = models_map.remove(&n.id).unwrap_or_default();
        let stripe_complete = stripe_map.get(&n.id).copied().unwrap_or(false);

        let probe_count = health.and_then(|h| h.probe_count).unwrap_or(0);
        let success_count = health.and_then(|h| h.success_count).unwrap_or(0);
        let uptime_pct = if probe_count > 0 {
            round2((success_count as f64 / probe_count as f64) * 100.0)
        } else {
            0.0
        };
        let avg_latency = health
            .and_then(|h| h.avg_latency_ms)
            .map(|l| (l * 10.0).round() / 10.0);

        let req_count = usage.and_then(|u| u.request_count).unwrap_or(0);
        let tokens_in = usage.and_then(|u| u.tokens_in).unwrap_or(0);
        let tokens_out = usage.and_then(|u| u.tokens_out).unwrap_or(0);
        let tokens_total = tokens_in + tokens_out;
        let earnings = round4(tokens_total as f64 * token_rate * revenue_share);

        node_stats.push(NodeProviderStats {
            node_id: n.id,
            node_name: n.name,
            gpu_name: n.gpu_name,
            vram_mb: n.vram_mb,
            status: n.status,
            uptime_pct_7d: uptime_pct,
            avg_latency_ms_7d: avg_latency,
            probe_count_7d: probe_count,
            request_count_7d: req_count,
            tokens_in_7d: tokens_in,
            tokens_out_7d: tokens_out,
            tokens_served_7d: tokens_total,
            estimated_earnings_usd_7d: earnings,
            stripe_onboarding_complete: stripe_complete,
            models,
        });
    }

    let totals = ProviderTotals {
        node_count: node_stats.len(),
        request_count_7d: node_stats.iter().map(|n| n.request_count_7d).sum(),
        tokens_served_7d: node_stats.iter().map(|n| n.tokens_served_7d).sum(),
        estimated_earnings_usd_7d: round4(
            node_stats
                .iter()
                .map(|n| n.estimated_earnings_usd_7d)
                .sum(),
        ),
    };

    Json(ProviderStatsResponse {
        nodes: node_stats,
        totals,
    })
    .into_response()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
