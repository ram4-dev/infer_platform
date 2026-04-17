# Infer Platform

Distributed AI inference platform that pools consumer GPU devices, shards LLMs across them, and exposes an OpenAI-compatible API.

**Live:** [infer.ram4.dev](https://infer.ram4.dev)

## Architecture

```
[Client] → OpenAI-compatible API Gateway (/v1/chat/completions)
               ↓
          Smart Router (pod-local → federation → cloud-fallback)
               ↓
          Pod Coordinator (Raft consensus)
               ↓
          Shard Ring (libp2p tensor streaming between GPU nodes)
```

## Monorepo Structure

```
crates/
  api-gateway/     # OpenAI-compatible HTTP server (Rust + Axum)
  node-agent/      # GPU node daemon (Rust)
web/               # Status dashboard (Next.js)
```

## Quick Start

### API Gateway

```bash
cd crates/api-gateway
cp .env.example .env
# Edit .env: set INFER_API_KEYS and OLLAMA_URL
cargo run
```

The gateway listens on `http://0.0.0.0:8080`.

### Node Agent

```bash
cd crates/node-agent
cp .env.example .env
# Edit .env: set COORDINATOR_URL and NODE_NAME
cargo run
```

### Web Dashboard

```bash
cd web
npm install
npm run dev
```

## API

### Chat Completions

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer pk_your_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

### List Models

```bash
curl http://localhost:8080/v1/models \
  -H "Authorization: Bearer pk_your_key"
```

### Node Registration (Internal)

```bash
curl http://localhost:8080/v1/internal/nodes \
  -H "Authorization: Bearer internal_secret" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-node", "host": "192.168.1.100", "port": 11434, "vram_mb": 8192, "gpu_name": "RTX 3080"}'
```

## Tech Stack

- **API Gateway / Node Agent**: Rust 1.95+, Axum 0.7, Tokio
- **Web Dashboard**: Next.js 15, Tailwind CSS, shadcn/ui
- **Inference Backend**: Ollama (MVP), direct llama.cpp (Phase 2)
- **P2P**: libp2p (Phase 2)
- **Database**: PostgreSQL + Redis (Phase 2)
