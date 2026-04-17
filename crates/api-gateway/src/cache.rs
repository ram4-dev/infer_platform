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


}
