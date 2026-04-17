/// ShardCoordinator: selects compute nodes for a request using the shard planner
/// and orchestrates inference execution across the chosen node(s).
///
/// Execution model (MVP):
/// - Single-node fit  → proxy request directly to that node's Ollama endpoint.
/// - Multi-node plan  → send to the controller node's `/infer/shard` agent endpoint,
///   which receives the full shard plan and chains through the pipeline itself.
///
/// True tensor-parallel layer splitting requires a llama.cpp RPC backend on each node.
/// The pipeline protocol below is structurally correct and ready for that upgrade.
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use shard_planner::{plan_shards, ModelRegistry, NodeCapacity, ShardPlan};

use crate::cache::NodeStats;
use crate::nodes::{NodeInfo, NodeStatus};
use crate::routes::chat::{ChatCompletionRequest, ChatMessage};

/// Serialised plan forwarded to the first node's agent when multi-node is required.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShardExecutionRequest {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub plan: ShardPlan,
}

pub struct ShardCoordinator {
    client: Client,
    /// Monotonic counter for round-robin load balancing across peer nodes.
    rr_counter: AtomicUsize,
}

impl ShardCoordinator {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("failed to build HTTP client"),
            rr_counter: AtomicUsize::new(0),
        }
    }

    /// Build an ordered list of candidate shard plans for load balancing and failover.
    ///
    /// When multiple nodes can independently serve the model (single-node fit), each
    /// gets its own plan.  Plans are sorted by p50 latency (fastest first) then rotated
    /// by a round-robin counter so traffic distributes evenly across peers over time.
    ///
    /// Falls back to a single multi-node plan when no node can fit the model alone.
    /// Returns an empty vec when there are no online nodes or insufficient total VRAM.
    pub fn build_candidates(
        &self,
        model: &str,
        nodes: &[NodeInfo],
        stats: &HashMap<String, NodeStats>,
    ) -> Vec<ShardPlan> {
        let spec =
            ModelRegistry::get(model).unwrap_or_else(|| ModelRegistry::estimate(model, 4096));
        let needed = spec.total_vram_mb();

        let online: Vec<&NodeInfo> = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Online)
            .collect();

        // Nodes that can serve the model without splitting.
        let mut solo: Vec<&NodeInfo> = online
            .iter()
            .copied()
            .filter(|n| n.vram_mb >= needed)
            .collect();

        if !solo.is_empty() {
            // Fastest first; VRAM DESC as tie-break.
            solo.sort_by(|a, b| {
                let pa = stats.get(&a.id).map(|s| s.p50_ms).unwrap_or(f64::MAX);
                let pb = stats.get(&b.id).map(|s| s.p50_ms).unwrap_or(f64::MAX);
                pa.partial_cmp(&pb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.vram_mb.cmp(&a.vram_mb))
            });

            // Apply round-robin rotation so each call starts from a different node.
            let n = solo.len();
            let offset = self.rr_counter.fetch_add(1, Ordering::Relaxed) % n;
            solo.rotate_left(offset);

            let candidates: Vec<ShardPlan> = solo
                .iter()
                .filter_map(|node| {
                    let cap = vec![NodeCapacity {
                        node_id: node.id.clone(),
                        host: node.host.clone(),
                        ollama_port: node.port,
                        agent_port: node.agent_port,
                        available_vram_mb: node.vram_mb,
                    }];
                    plan_shards(&spec, &cap).ok()
                })
                .collect();

            if !candidates.is_empty() {
                info!(
                    model,
                    candidates = candidates.len(),
                    "load balancing across single-node candidates"
                );
                return candidates;
            }
        }

        // Fall back to a single multi-node plan.
        if let Some(plan) = self.build_plan(model, nodes, stats) {
            return vec![plan];
        }

        vec![]
    }

    /// Execute with automatic failover across candidate plans.
    ///
    /// Tries each candidate in order.  On node failure, logs a warning and advances to
    /// the next candidate.  Returns an error only when every candidate has been exhausted.
    pub async fn execute_with_failover(
        &self,
        req: &ChatCompletionRequest,
        candidates: &[ShardPlan],
        request_id: &str,
    ) -> Result<serde_json::Value> {
        if candidates.is_empty() {
            anyhow::bail!("no inference nodes available for model '{}'", req.model);
        }

        let mut last_err = anyhow::anyhow!("all candidates failed");

        for (attempt, plan) in candidates.iter().enumerate() {
            let node_id = plan.controller().node_id.clone();
            match self.execute(req, plan, request_id).await {
                Ok(result) => {
                    if attempt > 0 {
                        info!(request_id, attempt, node = node_id, "failover succeeded");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    warn!(
                        request_id,
                        attempt,
                        node = node_id,
                        error = %e,
                        "node failed, trying next candidate"
                    );
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    /// Build a shard plan for `model` against the currently online nodes.
    ///
    /// Nodes are pre-sorted by (VRAM DESC, p50 ASC) before being passed to the
    /// planner.  Since the planner uses a stable sort on VRAM, equal-VRAM nodes
    /// preserve the p50 ordering, so the lowest-latency node among equals
    /// becomes the controller.  Degraded and offline nodes are excluded.
    ///
    /// Returns `None` if no online nodes are available or VRAM is insufficient.
    pub fn build_plan(
        &self,
        model: &str,
        nodes: &[NodeInfo],
        stats: &HashMap<String, NodeStats>,
    ) -> Option<ShardPlan> {
        let mut candidates: Vec<&NodeInfo> = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Online)
            .collect();

        // Sort (VRAM DESC, p50 ASC) — planner's stable VRAM sort preserves p50
        // ordering for equal-VRAM nodes.
        candidates.sort_by(|a, b| {
            b.vram_mb.cmp(&a.vram_mb).then_with(|| {
                let pa = stats.get(&a.id).map(|s| s.p50_ms).unwrap_or(f64::MAX);
                let pb = stats.get(&b.id).map(|s| s.p50_ms).unwrap_or(f64::MAX);
                pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        let capacities: Vec<NodeCapacity> = candidates
            .into_iter()
            .map(|n| NodeCapacity {
                node_id: n.id.clone(),
                host: n.host.clone(),
                ollama_port: n.port,
                agent_port: n.agent_port,
                available_vram_mb: n.vram_mb,
            })
            .collect();

        if capacities.is_empty() {
            return None;
        }

        let spec =
            ModelRegistry::get(model).unwrap_or_else(|| ModelRegistry::estimate(model, 4096));

        match plan_shards(&spec, &capacities) {
            Ok(plan) => {
                info!(model, nodes = plan.assignments.len(), "shard plan created");
                Some(plan)
            }
            Err(e) => {
                warn!("shard planning failed: {e}");
                None
            }
        }
    }

    /// Execute a non-streaming chat completion via the shard plan.
    pub async fn execute(
        &self,
        req: &ChatCompletionRequest,
        plan: &ShardPlan,
        request_id: &str,
    ) -> Result<serde_json::Value> {
        if plan.is_single_node() {
            self.single_node_execute(req, plan, request_id).await
        } else {
            self.pipeline_execute(req, plan, request_id).await
        }
    }

    // ── Single-node path ──────────────────────────────────────────────────────

    async fn single_node_execute(
        &self,
        req: &ChatCompletionRequest,
        plan: &ShardPlan,
        _request_id: &str,
    ) -> Result<serde_json::Value> {
        let node = plan.controller();
        let url = format!("{}/api/chat", node.ollama_base_url());

        let body = build_ollama_request(req);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to reach node {} at {url}", node.node_id))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("node {} returned {status}: {body}", node.node_id);
        }

        resp.json::<serde_json::Value>()
            .await
            .context("failed to parse Ollama response")
    }

    // ── Multi-node pipeline path ───────────────────────────────────────────────

    async fn pipeline_execute(
        &self,
        req: &ChatCompletionRequest,
        plan: &ShardPlan,
        request_id: &str,
    ) -> Result<serde_json::Value> {
        let controller = plan.controller();
        let url = format!("{}/infer/shard", controller.agent_base_url());

        let shard_req = ShardExecutionRequest {
            request_id: request_id.to_string(),
            model: req.model.clone(),
            messages: req.messages.clone(),
            stream: false,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            plan: plan.clone(),
        };

        info!(
            request_id,
            controller = controller.node_id,
            shards = plan.assignments.len(),
            "dispatching pipeline shard request"
        );

        let resp = self
            .client
            .post(&url)
            .json(&shard_req)
            .send()
            .await
            .with_context(|| {
                format!("failed to reach controller {} at {url}", controller.node_id)
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("controller returned {status}: {body}");
        }

        resp.json::<serde_json::Value>()
            .await
            .context("failed to parse shard response from controller")
    }
}

fn build_ollama_request(req: &ChatCompletionRequest) -> serde_json::Value {
    let mut body = json!({
        "model": req.model,
        "messages": req.messages,
        "stream": false,
    });

    let opts = build_ollama_options(req);
    if !opts.is_null() {
        body["options"] = opts;
    }
    body
}

fn build_ollama_options(req: &ChatCompletionRequest) -> serde_json::Value {
    let mut opts = json!({});
    if let Some(t) = req.temperature {
        opts["temperature"] = json!(t);
    }
    if let Some(p) = req.top_p {
        opts["top_p"] = json!(p);
    }
    if let Some(m) = req.max_tokens {
        opts["num_predict"] = json!(m);
    }
    if opts.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return serde_json::Value::Null;
    }
    opts
}
