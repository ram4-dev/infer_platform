# Architecture

This document covers design decisions, component responsibilities, data flows, and future evolution for the Infer Platform.

---

## Table of Contents

- [System Overview](#system-overview)
- [Component Responsibilities](#component-responsibilities)
- [Shard Planning Algorithm](#shard-planning-algorithm)
- [Request Lifecycle](#request-lifecycle)
- [Storage Strategy](#storage-strategy)
- [Authentication and Authorization](#authentication-and-authorization)
- [Rate Limiting](#rate-limiting)
- [Node Lifecycle](#node-lifecycle)
- [Graceful Degradation](#graceful-degradation)
- [Current Limitations](#current-limitations)
- [Phase 2 Roadmap](#phase-2-roadmap)

---

## System Overview

Infer Platform aggregates consumer GPU devices into a unified inference pool. The key insight is that most large models won't fit on a single consumer GPU, but they can be sharded across several. The platform hides this complexity behind a standard OpenAI-compatible API.

```
┌──────────────────────────────────────────────────────────────┐
│                         Clients                              │
│        (OpenAI SDK, curl, any HTTP client)                   │
└────────────────────────┬─────────────────────────────────────┘
                         │ POST /v1/chat/completions
                         ▼
┌──────────────────────────────────────────────────────────────┐
│                     API Gateway (Rust)                        │
│                                                              │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │ Auth     │  │ Rate Limiter │  │   Shard Coordinator  │   │
│  │ (SHA-256)│  │ (Redis)      │  │   (plan → execute)   │   │
│  └──────────┘  └──────────────┘  └──────────────────────┘   │
│                                                              │
│  ┌──────────┐  ┌──────────────┐                             │
│  │ Node     │  │ Key Store    │                             │
│  │ Registry │  │ (PostgreSQL) │                             │
│  └──────────┘  └──────────────┘                             │
└──────────────────────┬───────────────────────────────────────┘
                       │ shard forward
            ┌──────────┴──────────┐
            │                     │
            ▼                     ▼
┌───────────────────┐   ┌───────────────────┐
│   Node Agent A    │   │   Node Agent B    │
│   (controller)    │──▶│   (worker)        │  ...
│                   │   │                   │
│  ┌─────────────┐  │   │  ┌─────────────┐  │
│  │   Ollama    │  │   │  │   Ollama    │  │
│  │  (GPU x)    │  │   │  │  (GPU y)    │  │
│  └─────────────┘  │   │  └─────────────┘  │
└───────────────────┘   └───────────────────┘

              ┌─────────────────────┐
              │   Web Dashboard     │
              │   (Next.js/Vercel)  │
              │  reads internal API │
              └─────────────────────┘
```

---

## Component Responsibilities

### API Gateway (`crates/api-gateway`)

Single entry point for all client and internal traffic. Owns:

- **Public API surface** — `/v1/chat/completions`, `/v1/models`
- **Authentication** — per-request API key validation
- **Rate limiting** — per-key fixed-window RPM counters
- **Node registry** — in-memory + PostgreSQL-backed list of live GPU nodes
- **Shard coordination** — selects a plan, dispatches to controller node, waits for response
- **Key management** — CRUD for API keys via internal endpoints
- **Stale node sweeper** — background task that removes nodes not seen for >2 minutes

The gateway is stateless between requests (all persistent state lives in PostgreSQL/Redis), making it horizontally scalable.

### Node Agent (`crates/node-agent`)

Lightweight daemon that runs on each GPU machine. Owns:

- **Hardware reporting** — GPU name and available VRAM via `sysinfo`
- **Registration heartbeat** — 30-second loop to keep node visible in gateway's registry
- **Shard execution** — receives `ShardForwardRequest` from coordinator, runs inference on local Ollama, chains to next node
- **Local Ollama proxy** — translates platform shard requests to Ollama's `/api/chat`

The agent does not store any persistent state. Restarts are safe — re-registration is automatic.

### Shard Planner (`crates/shard-planner`)

Pure library (no I/O, no runtime deps). Given a model spec and a list of node capacities, produces a contiguous layer assignment:

- Largest-VRAM node is always the controller (shard index 0), because it handles embedding/KV-cache overhead
- Layers assigned greedily from controller outward until all layers are placed
- Returns `InsufficientVram` error if total available VRAM < model requirement
- Returns `NoNodes` if the node list is empty

### Web Dashboard (`web/`)

Next.js status page deployed on Vercel. Server-side route handlers proxy to the gateway's internal endpoints (node list, model list) and render:

- Live node table (no cache — always fresh)
- Model availability (60-second revalidation)
- Summary stats (nodes online, total VRAM, available models)

---

## Shard Planning Algorithm

The planner runs entirely inside the gateway per request. It is cheap — O(n log n) on node count, no network I/O.

```
Input:
  model: ModelSpec { total_layers, vram_per_layer_mb, context_vram_mb }
  nodes: Vec<NodeCapacity> { available_vram_mb, ... }

Algorithm:
  1. Check nodes not empty → NoNodes error
  2. Sum available_vram_mb across all nodes
  3. Compare sum to model.total_vram_mb() = total_layers * vram_per_layer_mb + context_vram_mb
     → InsufficientVram if not enough
  4. Sort nodes by available_vram_mb descending
     (largest node = controller = handles context overhead)
  5. Deduct context_vram_mb from first node's effective capacity
  6. For each node in sorted order:
       layers_for_node = floor((available - deducted_overhead) / vram_per_layer_mb)
       assign [cursor, cursor + layers_for_node) to this node
       advance cursor
  7. Assert cursor == total_layers (covered all layers)
  8. Return ShardPlan { assignments: Vec<ShardAssignment> }
```

**Why greedy?** The assignment problem here is simpler than bin-packing: layers are ordered (you can't skip), and we want minimal coordination overhead, so the controller doing the most work (largest chunk) is preferred. Greedy works well for this.

**Model registry:** The planner ships a hardcoded table (~18 popular models) with empirically measured VRAM-per-layer values. Models not in the table fall back to estimation based on total guessed VRAM (32 layers assumed). This table should grow over time.

---

## Request Lifecycle

### Streaming request (happy path, multi-node)

```
Client                   Gateway              Controller Node       Worker Node
  │                         │                      │                    │
  │ POST /v1/chat/...       │                      │                    │
  │────────────────────────▶│                      │                    │
  │                         │ validate key         │                    │
  │                         │ check rate limit     │                    │
  │                         │ build shard plan     │                    │
  │                         │                      │                    │
  │                         │ POST /infer/shard    │                    │
  │                         │ {plan, messages}     │                    │
  │                         │─────────────────────▶│                    │
  │                         │                      │ POST /api/chat     │
  │                         │                      │ (Ollama)           │
  │                         │                      │──▶ (local)         │
  │                         │                      │◀── response        │
  │                         │                      │                    │
  │                         │                      │ POST /infer/shard  │
  │                         │                      │ (append assistant) │
  │                         │                      │───────────────────▶│
  │                         │                      │                    │ (Ollama)
  │                         │                      │                    │──▶
  │                         │                      │◀── final response ─│
  │                         │                      │                    │
  │◀── SSE stream ──────────│◀── response ─────────│                    │
```

**Current MVP note:** Streaming from the gateway proxies the controller node's Ollama SSE directly. Full pipeline streaming (stitching SSE across multiple nodes in sequence) is deferred to Phase 2.

### Single-node path

When the shard plan produces a single assignment (model fits on one node), the gateway proxies directly to that node's Ollama endpoint. No shard forwarding involved — lower overhead.

---

## Storage Strategy

The platform supports two storage modes selected by environment at startup:

### Production mode (both DATABASE_URL and REDIS_URL set)

- **PostgreSQL** stores API keys (SHA-256 hashed), node registry, and audit trail
- **Redis** stores per-key RPM rate limit counters (TTL-based fixed windows)
- SQLx migrations run automatically on gateway startup
- Background sweeper removes nodes with `last_seen` older than 2 minutes

### Development mode (no DATABASE_URL)

- API keys loaded from `INFER_API_KEYS` comma-separated env var at startup
- Node registry held in `Arc<RwLock<HashMap>>` in process memory
- No rate limiting (Redis absent → fails open, logs warning)
- Node state lost on gateway restart

This dual-mode design allows zero-dependency local development while being production-ready with external services.

### Database schema

```sql
CREATE TABLE api_keys (
  id              UUID PRIMARY KEY,
  key_hash        VARCHAR(64) NOT NULL UNIQUE,  -- SHA-256 hex of plaintext key
  owner           VARCHAR     NOT NULL,
  rate_limit_rpm  INT         NOT NULL DEFAULT 60,
  daily_spend_cap_cents INT,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  revoked_at      TIMESTAMPTZ
);

CREATE TABLE nodes (
  id            UUID PRIMARY KEY,
  name          VARCHAR     NOT NULL UNIQUE,
  host          VARCHAR     NOT NULL,
  port          INT         NOT NULL,     -- Ollama port
  agent_port    INT         NOT NULL,     -- Agent API port
  gpu_name      VARCHAR     NOT NULL,
  vram_mb       BIGINT      NOT NULL,
  status        VARCHAR     NOT NULL DEFAULT 'online',
  registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen     TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

---

## Authentication and Authorization

### Client API Keys

- Keys generated as random 32-byte values, encoded as hex (prefix `pk_`)
- Stored as SHA-256 hash in `api_keys.key_hash` — plaintext never persisted
- Per-request: hash the incoming Bearer token, look up in DB (or in-memory map for dev)
- Revoked keys have `revoked_at` set; lookup rejects them

### Internal Key

- Single shared secret (`INFER_INTERNAL_KEY`) for node registration and key management
- Not hashed — direct string equality comparison
- Should be a long random secret in production (default `internal_dev_secret` is for dev only)
- Internal endpoints: `POST /v1/internal/nodes`, `GET /v1/internal/nodes`, `POST/GET/DELETE /v1/internal/keys`

---

## Rate Limiting

Fixed-window counter per key per minute, backed by Redis:

- Key: `rate:{key_id}:{unix_minute}`
- TTL: 61 seconds (window + 1s buffer)
- On each request: `INCR` → compare to `rate_limit_rpm`
- If Redis is unreachable: request is **allowed** (fail-open) with a log warning

This means rate limiting is best-effort in degraded Redis scenarios, which is the right tradeoff for inference availability.

---

## Node Lifecycle

```
Node Agent starts
       │
       ▼
Detect hardware (GPU name, VRAM)
       │
       ▼
Registration loop (exponential backoff: 2s → 120s max)
  POST /v1/internal/nodes
       │
       ▼ (success)
30-second heartbeat (re-POST same registration payload)
       │
       ▼
Gateway updates nodes.last_seen
       │
Background sweeper (gateway side)
  every 60s: DELETE WHERE last_seen < now() - 2min
```

Node re-registration is an upsert on `name` — the same node restarting will update its record rather than create a duplicate.

---

## Graceful Degradation

The platform is designed to keep serving requests even when parts of the infrastructure are missing:

| Missing component | Behavior |
|---|---|
| PostgreSQL | Falls back to in-memory keys + node registry |
| Redis | Rate limiting disabled, requests pass through |
| All GPU nodes | Falls back to `OLLAMA_URL` (single Ollama instance) |
| Some GPU nodes | Shard plan recalculates with remaining online nodes |
| Node agent restart | Re-registers within 2 seconds, heartbeat resumes |

---

## Current Limitations

### Ollama can't do layer-range inference

The biggest limitation: Ollama runs the full model on a single GPU. The multi-node pipeline in the current MVP sends the full prompt to the controller's Ollama, gets a response, then forwards that response to worker nodes — which is **sequential inference, not true tensor parallelism**.

The architecture is in place for real sharding (layer-range assignment, `ShardForwardRequest` with `layer_start/layer_end`, chaining between agents), but activating it requires switching to `llama.cpp` with `--rpc` mode, which exposes per-layer inference.

### Streaming across nodes

Current multi-node streaming proxies the controller's Ollama SSE stream directly to the client. Worker node outputs are appended in a subsequent non-streaming pass. Full end-to-end SSE streaming across the pipeline is Phase 2.

### No authentication on node agents

Node agents trust any caller presenting the internal key. There is no TLS between gateway and agents, no mutual authentication, and no request signing. This is safe on a private LAN but not on the public internet.

---

## Phase 2 Roadmap

| Feature | Why |
|---|---|
| `llama.cpp --rpc` backend | True tensor parallelism across GPU shards |
| libp2p peer discovery | Nodes find each other without central coordinator |
| mTLS between gateway ↔ agents | Secure inter-node communication |
| Full pipeline SSE streaming | End-to-end streaming with multi-node |
| Spend tracking | `daily_spend_cap_cents` enforcement (currently stored, not enforced) |
| Model download orchestration | Pull models across nodes on demand |
| Multi-gateway federation | Horizontal gateway scaling with shared Redis |
