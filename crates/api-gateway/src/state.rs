use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock};

use crate::billing::StripeClient;
use crate::cache::{LatencyCache, NodeStats, RateLimiter};
use crate::nodes::NodeInfo;
use crate::shard_coordinator::ShardCoordinator;

pub struct StripeConfig {
    pub client: StripeClient,
    pub webhook_secret: String,
    /// Stripe Meters API event name (configured on the Meter in Stripe Dashboard).
    pub meter_event_name: String,
    /// Stripe Price ID for the metered subscription (created in Stripe Dashboard).
    pub price_id: Option<String>,
    /// USD price per 1 000 tokens (default 0.002 = $0.002/1K).
    pub token_rate_usd_per_1k: f64,
    /// Platform commission rate (default 0.20 = 20%).
    pub commission_rate: f64,
}

pub struct AppState {
    /// Env-var API keys — fallback when DATABASE_URL is not set (dev mode).
    pub api_keys: HashSet<String>,
    pub internal_key: String,
    /// Default Ollama URL used when no nodes are registered.
    pub ollama_url: String,
    /// In-memory node store (active when DATABASE_URL is not set).
    pub nodes: Arc<RwLock<Vec<NodeInfo>>>,
    pub coordinator: ShardCoordinator,
    /// Production persistence — None in single-node dev mode.
    pub db: Option<sqlx::PgPool>,
    /// Redis-backed rate limiter — None when REDIS_URL is not set.
    pub rate_limiter: Option<Arc<Mutex<RateLimiter>>>,
    /// Redis-backed latency stat cache — None when REDIS_URL is not set.
    pub latency_cache: Option<Arc<Mutex<LatencyCache>>>,
    /// In-process per-node latency stats updated by the health monitor.
    pub node_stats: Arc<RwLock<HashMap<String, NodeStats>>>,
    /// Stripe billing config — None when STRIPE_SECRET_KEY is not set.
    pub stripe: Option<Arc<StripeConfig>>,
}

impl AppState {
    pub async fn from_env() -> Result<Self> {
        let raw_keys = std::env::var("INFER_API_KEYS").unwrap_or_default();
        let api_keys: HashSet<String> = raw_keys
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let internal_key = std::env::var("INFER_INTERNAL_KEY")
            .unwrap_or_else(|_| "internal_dev_secret".to_string());

        let ollama_url =
            std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

        let db = if let Ok(url) = std::env::var("DATABASE_URL") {
            let pool = crate::db::init_pool(&url).await?;
            crate::db::run_migrations(&pool).await?;
            tracing::info!("PostgreSQL connected — migrations applied");
            Some(pool)
        } else {
            tracing::warn!("DATABASE_URL not set — using in-memory node storage (dev mode)");
            None
        };

        let (rate_limiter, latency_cache) = if let Ok(url) = std::env::var("REDIS_URL") {
            let client = redis::Client::open(url).context("invalid REDIS_URL")?;
            let conn = redis::aio::ConnectionManager::new(client)
                .await
                .context("failed to connect to Redis")?;
            tracing::info!("Redis connected — rate limiting and latency cache enabled");
            let rl = Some(Arc::new(Mutex::new(RateLimiter::new(conn.clone()))));
            let lc = Some(Arc::new(Mutex::new(LatencyCache::new(conn))));
            (rl, lc)
        } else {
            tracing::warn!("REDIS_URL not set — rate limiting and latency cache disabled");
            (None, None)
        };

        let stripe = if let Ok(secret_key) = std::env::var("STRIPE_SECRET_KEY") {
            let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
                .unwrap_or_default();
            let meter_event_name = std::env::var("STRIPE_METER_EVENT_NAME")
                .unwrap_or_else(|_| "tokens_used".to_string());
            let price_id = std::env::var("STRIPE_PRICE_ID").ok();
            let token_rate_usd_per_1k = std::env::var("TOKEN_RATE_USD_PER_1K")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.002_f64);
            let commission_rate = std::env::var("COMMISSION_RATE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.20_f64);

            tracing::info!("Stripe billing enabled (meter={meter_event_name})");
            Some(Arc::new(StripeConfig {
                client: StripeClient::new(secret_key),
                webhook_secret,
                meter_event_name,
                price_id,
                token_rate_usd_per_1k,
                commission_rate,
            }))
        } else {
            tracing::warn!("STRIPE_SECRET_KEY not set — billing disabled");
            None
        };

        if db.is_none() && api_keys.is_empty() {
            anyhow::bail!(
                "INFER_API_KEYS must be set when DATABASE_URL is not configured (dev mode)"
            );
        }

        Ok(Self {
            api_keys,
            internal_key,
            ollama_url,
            nodes: Arc::new(RwLock::new(Vec::new())),
            coordinator: ShardCoordinator::new(),
            db,
            rate_limiter,
            latency_cache,
            node_stats: Arc::new(RwLock::new(HashMap::new())),
            stripe,
        })
    }

    pub fn is_valid_internal_key(&self, key: &str) -> bool {
        key == self.internal_key
    }
}
