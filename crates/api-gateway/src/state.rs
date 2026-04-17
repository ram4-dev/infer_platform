use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock};

use crate::cache::RateLimiter;
use crate::nodes::NodeInfo;
use crate::shard_coordinator::ShardCoordinator;

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

        let rate_limiter = if let Ok(url) = std::env::var("REDIS_URL") {
            let client = redis::Client::open(url).context("invalid REDIS_URL")?;
            let conn = redis::aio::ConnectionManager::new(client)
                .await
                .context("failed to connect to Redis")?;
            tracing::info!("Redis connected — rate limiting enabled");
            Some(Arc::new(Mutex::new(RateLimiter::new(conn))))
        } else {
            tracing::warn!("REDIS_URL not set — rate limiting disabled");
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
        })
    }

    pub fn is_valid_internal_key(&self, key: &str) -> bool {
        key == self.internal_key
    }
}
