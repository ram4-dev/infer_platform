use std::sync::Arc;
use std::time::Duration;

use crate::nodes::NodeStatus;
use crate::state::AppState;

/// Spawns a background task that marks nodes as offline when their last heartbeat
/// is older than 90 seconds. Runs every 30 seconds.
pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            sweep(&state).await;
        }
    });
}

async fn sweep(state: &AppState) {
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(90);

    if let Some(ref pool) = state.db {
        match sqlx::query(
            "UPDATE nodes SET status = 'offline' \
             WHERE last_seen < $1 AND status != 'offline'",
        )
        .bind(cutoff)
        .execute(pool)
        .await
        {
            Ok(r) if r.rows_affected() > 0 => {
                tracing::info!("Stale sweep: {} node(s) marked offline", r.rows_affected());
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Stale node sweep failed: {e}"),
        }
    } else {
        let mut nodes = state.nodes.write().await;
        for node in nodes.iter_mut() {
            if node.last_seen < cutoff && node.status != NodeStatus::Offline {
                node.status = NodeStatus::Offline;
                tracing::info!("Node '{}' marked offline (stale heartbeat)", node.name);
            }
        }
    }
}
