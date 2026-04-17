use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

/// Per-node latency and uptime stats, cached in Redis and in-process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub uptime_7d: f64,
}

/// Redis-backed latency stat cache (TTL 60 s per node).
pub struct LatencyCache {
    conn: ConnectionManager,
}

impl LatencyCache {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    pub async fn set_node_stats(&mut self, node_id: &str, stats: &NodeStats) -> Result<()> {
        let key = format!("health:{node_id}:stats");
        let val = serde_json::to_string(stats)?;
        let _: () = self.conn.set_ex(&key, val, 60u64).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_node_stats(&mut self, node_id: &str) -> Result<Option<NodeStats>> {
        let key = format!("health:{node_id}:stats");
        let val: Option<String> = self.conn.get(&key).await?;
        Ok(val.and_then(|s| serde_json::from_str(&s).ok()))
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    conn: ConnectionManager,
}

impl RateLimiter {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Increments the fixed-window counter for this key and returns whether it is within limits.
    pub async fn check_and_increment(&mut self, key_id: &str, rpm: i64) -> Result<bool> {
        let minute = chrono::Utc::now().timestamp() / 60;
        let redis_key = format!("rate:{key_id}:{minute}");

        let count: i64 = self.conn.incr(&redis_key, 1i64).await?;
        if count == 1 {
            let _: () = self.conn.expire(&redis_key, 120).await?;
        }

        Ok(count <= rpm)
    }
}
