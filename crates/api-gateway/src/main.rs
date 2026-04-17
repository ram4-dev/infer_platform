mod auth;
mod cache;
mod db;
mod models;
mod nodes;
mod routes;
mod shard_coordinator;
mod stale_nodes;
mod state;

use std::sync::Arc;

use anyhow::Result;
use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = Arc::new(AppState::from_env().await?);
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let addr = format!("0.0.0.0:{port}");

    stale_nodes::spawn(state.clone());

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("infer API gateway listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: Arc<AppState>) -> Router {
    let authed = Router::new()
        .route("/v1/chat/completions", post(routes::chat::completions))
        .route("/v1/models", get(routes::models::list))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_key,
        ));

    let internal = Router::new()
        .route("/v1/internal/nodes", post(routes::nodes::register))
        .route("/v1/internal/nodes", get(routes::nodes::list))
        .route("/v1/internal/keys", post(routes::keys::create))
        .route("/v1/internal/keys", get(routes::keys::list))
        .route("/v1/internal/keys/:id", delete(routes::keys::revoke))
        .route("/v1/internal/usage", get(routes::usage::summary))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_internal_key,
        ));

    Router::new()
        .merge(authed)
        .merge(internal)
        .route("/ping", get(|| async { "pong" }))
        .route("/health", get(routes::health::check))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
