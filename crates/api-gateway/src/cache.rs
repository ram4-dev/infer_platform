use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

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

    /// Cache a serialized value with TTL (seconds).
    pub async fn set_cached(&mut self, key: &str, value: &str, ttl_secs: u64) -> Result<()> {
        let _: () = self.conn.set_ex(key, value, ttl_secs).await?;
        Ok(())
    }

    /// Retrieve a cached value.
    pub async fn get_cached(&mut self, key: &str) -> Result<Option<String>> {
        let val: Option<String> = self.conn.get(key).await?;
        Ok(val)
    }

    /// Invalidate a cache key.
    pub async fn invalidate(&mut self, key: &str) -> Result<()> {
        let _: () = self.conn.del(key).await?;
        Ok(())
    }
}
