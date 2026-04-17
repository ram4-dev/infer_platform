# Infer Platform

Distributed AI inference platform that pools consumer GPU devices, shards LLMs across them, and exposes an OpenAI-compatible API.

**Live:** [infer.ram4.dev](https://infer.ram4.dev)

---

## Table of Contents

- [Architecture](#architecture)
- [Monorepo Structure](#monorepo-structure)
- [Quick Start](#quick-start)
  - [Docker Compose (recommended)](#docker-compose-recommended)
  - [Manual Setup](#manual-setup)
- [Configuration](#configuration)
  - [API Gateway](#api-gateway-env)
  - [Node Agent](#node-agent-env)
  - [Web Dashboard](#web-dashboard-env)
- [API Reference](#api-reference)
  - [Chat Completions](#chat-completions)
  - [List Models](#list-models)
  - [Node Registration](#node-registration-internal)
  - [API Key Management](#api-key-management-internal)
- [Adding a GPU Node](#adding-a-gpu-node)
- [Supported Models](#supported-models)
- [Tech Stack](#tech-stack)

---

## Architecture

```
Client Request
     │
     ▼
┌─────────────────────────────────────┐
│         API Gateway (:8080)         │
│  • OpenAI-compatible HTTP API       │
│  • API key auth + rate limiting     │
│  • Shard planner (greedy VRAM fit)  │
│  • Node registry (PostgreSQL)       │
└──────────────┬──────────────────────┘
               │ shard plan
     ┌─────────┴─────────┐
     │                   │
     ▼                   ▼
┌──────────┐       ┌──────────┐
│  Node A  │──────▶│  Node B  │  ...
│ (GPU x)  │ chain │ (GPU y)  │
│ Ollama   │       │ Ollama   │
└──────────┘       └──────────┘
```

**Request flow:**
1. Client sends an OpenAI-compatible request with a Bearer token.
2. Gateway validates the key (hashed SHA-256 lookup against PostgreSQL or in-memory list).
3. Redis rate limiter enforces per-key RPM limits (fails open if Redis is down).
4. Shard planner allocates model layers across available nodes by VRAM capacity (greedy, largest node first as controller).
5. Single-node path: gateway proxies directly to that node's Ollama instance.
6. Multi-node path: gateway sends to the controller node's `/infer/shard` endpoint with the full plan; each node chains to the next.
7. Response is formatted as OpenAI JSON or SSE stream and returned to the client.

---

## Monorepo Structure

```
crates/
  api-gateway/     # OpenAI-compatible HTTP server (Rust + Axum)
  node-agent/      # GPU node daemon (Rust)
  shard-planner/   # Stateless layer-assignment library (Rust)
web/               # Status dashboard (Next.js)
docker-compose.yml # Full local stack
Dockerfile         # Multi-stage build (gateway + agent)
```

---

## Quick Start

### Docker Compose (recommended)

```bash
# Start PostgreSQL, Redis, API gateway, and one node agent
docker compose up --build

# Gateway available at http://localhost:8080
# Node agent at http://localhost:8181
# Status dashboard: cd web && npm run dev (http://localhost:3000)
```

The default compose stack uses `internal_dev_secret` as the internal key and no API keys (unauthenticated — development only). Set `INFER_API_KEYS` to add static keys.

### Manual Setup

**Prerequisites:** Rust 1.82+, [Ollama](https://ollama.ai) running locally, Node.js 20+

**API Gateway**

```bash
cd crates/api-gateway
cp .env.example .env
# Edit .env — set at minimum INFER_API_KEYS
cargo run
# Listens on http://0.0.0.0:8080
```

**Node Agent** (run on each GPU machine)

```bash
cd crates/node-agent
cp .env.example .env
# Edit .env — set COORDINATOR_URL to gateway address
cargo run
# Listens on http://0.0.0.0:8181 and registers with gateway
```

**Web Dashboard**

```bash
cd web
cp .env.example .env.local
# Edit .env.local — set GATEWAY_URL and keys
npm install
npm run dev
# http://localhost:3000
```

---

## Configuration

### API Gateway (env) {#api-gateway-env}

| Variable | Default | Required | Description |
|---|---|---|---|
| `PORT` | `8080` | No | HTTP listen port |
| `INFER_API_KEYS` | — | No* | Comma-separated static API keys (dev fallback when no DB) |
| `INFER_INTERNAL_KEY` | `internal_dev_secret` | Yes (prod) | Bearer token for internal endpoints |
| `OLLAMA_URL` | `http://localhost:11434` | No | Fallback Ollama when no nodes registered |
| `DATABASE_URL` | — | No* | PostgreSQL DSN. Enables persistent keys + node registry |
| `REDIS_URL` | — | No | Redis DSN. Enables distributed rate limiting |
| `RUST_LOG` | `info` | No | Log filter (`api_gateway=debug,tower_http=info`) |

*Without `DATABASE_URL`, keys come from `INFER_API_KEYS` and node state is in-memory (lost on restart).

### Node Agent (env) {#node-agent-env}

| Variable | Default | Required | Description |
|---|---|---|---|
| `NODE_NAME` | system hostname | No | Display name shown in dashboard |
| `NODE_HOST` | `127.0.0.1` | Yes (remote) | IP address reported to coordinator |
| `NODE_PORT` | `11434` | No | Ollama port on this machine |
| `AGENT_PORT` | `8181` | No | Node agent HTTP port |
| `COORDINATOR_URL` | `http://localhost:8080` | Yes | API gateway URL |
| `INFER_INTERNAL_KEY` | `internal_dev_secret` | Yes (prod) | Must match gateway's internal key |
| `RUST_LOG` | `info` | No | Log filter |

### Web Dashboard (env) {#web-dashboard-env}

| Variable | Default | Description |
|---|---|---|
| `GATEWAY_URL` | `http://localhost:8080` | API gateway URL (server-side route handlers) |
| `GATEWAY_INTERNAL_KEY` | `internal_dev_secret` | Internal key for node list endpoint |
| `GATEWAY_API_KEY` | — | API key for `/v1/models` endpoint |

---

## API Reference

All public endpoints require `Authorization: Bearer <api_key>`.
Internal endpoints require `Authorization: Bearer <internal_key>`.

### Chat Completions

```http
POST /v1/chat/completions
Authorization: Bearer pk_your_key
Content-Type: application/json
```

**Request body** (OpenAI-compatible subset):

```json
{
  "model": "llama3.2",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": true,
  "max_tokens": 512,
  "temperature": 0.7
}
```

**Stream response** (`stream: true`): Server-Sent Events, `data: {...}` lines, terminated by `data: [DONE]`.

**Non-stream response:**

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1713000000,
  "model": "llama3.2",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "Hi there!"},
    "finish_reason": "stop"
  }],
  "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}
}
```

**Example:**

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer pk_your_key" \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3.2","messages":[{"role":"user","content":"Hello!"}],"stream":true}'
```

### List Models

```http
GET /v1/models
Authorization: Bearer pk_your_key
```

Returns models available on registered Ollama nodes.

```bash
curl http://localhost:8080/v1/models \
  -H "Authorization: Bearer pk_your_key"
```

### Node Registration (Internal)

Register or update a GPU node:

```http
POST /v1/internal/nodes
Authorization: Bearer internal_dev_secret
Content-Type: application/json
```

```json
{
  "name": "rtx-4090-node",
  "host": "192.168.1.100",
  "port": 11434,
  "agent_port": 8181,
  "vram_mb": 24576,
  "gpu_name": "NVIDIA RTX 4090"
}
```

List registered nodes:

```bash
curl http://localhost:8080/v1/internal/nodes \
  -H "Authorization: Bearer internal_dev_secret"
```

### API Key Management (Internal)

Create a key:

```http
POST /v1/internal/keys
Authorization: Bearer internal_dev_secret
Content-Type: application/json
```

```json
{
  "owner": "alice",
  "rate_limit_rpm": 60,
  "daily_spend_cap_cents": 1000
}
```

Response includes `key` (plaintext, shown once) and `id`.

List keys:

```bash
curl http://localhost:8080/v1/internal/keys \
  -H "Authorization: Bearer internal_dev_secret"
```

Revoke a key:

```bash
curl -X DELETE http://localhost:8080/v1/internal/keys/<id> \
  -H "Authorization: Bearer internal_dev_secret"
```

Health check:

```bash
curl http://localhost:8080/ping  # → "pong"
curl http://localhost:8080/health
```

---

## Adding a GPU Node

1. Install [Ollama](https://ollama.ai) on the GPU machine and pull your models:
   ```bash
   ollama pull llama3.2
   ```

2. Download and run the node agent binary (or build from source):
   ```bash
   NODE_NAME=my-gpu-node \
   NODE_HOST=<machine-ip> \
   COORDINATOR_URL=http://<gateway-ip>:8080 \
   INFER_INTERNAL_KEY=<your-internal-key> \
   ./node-agent
   ```

3. Verify registration:
   ```bash
   curl http://<gateway>:8080/v1/internal/nodes \
     -H "Authorization: Bearer <internal-key>"
   ```

The node auto-heartbeats every 30 seconds. The gateway sweeps stale nodes (no heartbeat for >2 minutes) automatically.

---

## Supported Models

The shard planner has built-in VRAM profiles for these models:

| Model | Layers | VRAM (approx) |
|---|---|---|
| `llama3.2` (3B) | 28 | 2 GB |
| `llama3.2:1b` | 16 | 1.3 GB |
| `llama3.1:8b` | 32 | 5 GB |
| `llama3.1:70b` | 80 | 42 GB |
| `mistral:7b` | 32 | 4.5 GB |
| `qwen2.5:72b` | 80 | 46 GB |
| `phi4` | 40 | 10 GB |
| `deepseek-r1:70b` | 80 | 43 GB |
| `gemma3:27b` | 62 | 17 GB |

Unknown models fall back to a VRAM-based estimation (32 layers assumed).

---

## Tech Stack

| Layer | Technology |
|---|---|
| API Gateway | Rust 1.82+, Axum 0.7, Tokio |
| Node Agent | Rust 1.82+, Axum 0.7, sysinfo |
| Shard Planner | Rust (library crate, no runtime deps) |
| Web Dashboard | Next.js 16, TypeScript, Tailwind CSS, shadcn/ui |
| Inference Backend | Ollama (MVP) · llama.cpp RPC (Phase 2) |
| Database | PostgreSQL 16 (SQLx 0.8) |
| Cache / Rate Limit | Redis 7 |
| P2P | libp2p (Phase 2) |
| Container | Docker multi-stage build |
| Dashboard Hosting | Vercel |
