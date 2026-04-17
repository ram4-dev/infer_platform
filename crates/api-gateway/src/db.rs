use anyhow::{Context, Result};
use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> Result<PgPool> {
    PgPool::connect(database_url)
        .await
        .context("failed to connect to PostgreSQL")
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("failed to run database migrations")
}
