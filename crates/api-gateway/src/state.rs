use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use crate::nodes::NodeInfo;
use crate::shard_coordinator::ShardCoordinator;

pub struct AppState {
    pub api_keys: HashSet<String>,
    pub internal_key: String,
    /// Fallback Ollama URL used when no nodes are registered.
    pub ollama_url: String,
    pub nodes: Arc<RwLock<Vec<NodeInfo>>>,
    pub coordinator: ShardCoordinator,
}

impl AppState {
    pub fn from_env() -> Result<Self> {
        let raw_keys = std::env::var("INFER_API_KEYS")
            .context("INFER_API_KEYS must be set (comma-separated, e.g. pk_abc,pk_def)")?;

        let api_keys = raw_keys
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<HashSet<_>>();

        if api_keys.is_empty() {
            anyhow::bail!("INFER_API_KEYS must contain at least one key");
        }

        let internal_key = std::env::var("INFER_INTERNAL_KEY")
            .unwrap_or_else(|_| "internal_dev_secret".to_string());

        let ollama_url = std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        Ok(Self {
            api_keys,
            internal_key,
            ollama_url,
            nodes: Arc::new(RwLock::new(Vec::new())),
            coordinator: ShardCoordinator::new(),
        })
    }

    pub fn is_valid_api_key(&self, key: &str) -> bool {
        self.api_keys.contains(key)
    }

    pub fn is_valid_internal_key(&self, key: &str) -> bool {
        key == self.internal_key
    }
}
