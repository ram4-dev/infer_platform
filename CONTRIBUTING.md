# Contributing

## Prerequisites

- Rust 1.82+ (`rustup update stable`)
- Node.js 20+ and npm
- Docker + Docker Compose (for local stack)
- [Ollama](https://ollama.ai) installed and running with at least one model pulled

## Local Development

### 1. Start dependencies

```bash
docker compose up postgres redis -d
```

### 2. Run the API gateway

```bash
cd crates/api-gateway
cat > .env <<'EOF'
DATABASE_URL=postgres://infer:infer_dev_password@localhost:5432/infer
REDIS_URL=redis://localhost:6379
INFER_INTERNAL_KEY=dev_secret
INFER_API_KEYS=dev_key_1
OLLAMA_URL=http://localhost:11434
RUST_LOG=api_gateway=debug,tower_http=info
EOF
cargo run
```

### 3. Run a node agent

```bash
cd crates/node-agent
cat > .env <<'EOF'
COORDINATOR_URL=http://localhost:8080
INFER_INTERNAL_KEY=dev_secret
NODE_NAME=local-dev-node
NODE_HOST=127.0.0.1
RUST_LOG=node_agent=debug
EOF
cargo run
```

### 4. Run the web dashboard

```bash
cd web
cat > .env.local <<'EOF'
GATEWAY_URL=http://localhost:8080
GATEWAY_INTERNAL_KEY=dev_secret
GATEWAY_API_KEY=dev_key_1
EOF
npm install
npm run dev
# http://localhost:3000
```

## Testing

### Rust tests

```bash
# All crates
cargo test --workspace

# Specific crate
cargo test -p shard-planner

# With logging
RUST_LOG=debug cargo test -p api-gateway
```

The shard planner has comprehensive unit tests covering greedy allocation, edge cases (single node, exact fit, insufficient VRAM), and model registry lookups. Run these first when modifying planning logic.

### Type checking (web)

```bash
cd web
npm run build   # catches TypeScript errors
```

### Integration smoke test

```bash
# Verify gateway is up
curl http://localhost:8080/ping

# Verify a node is registered
curl http://localhost:8080/v1/internal/nodes \
  -H "Authorization: Bearer dev_secret"

# Test a completion
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer dev_key_1" \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3.2","messages":[{"role":"user","content":"ping"}],"max_tokens":10}'
```

## Project Structure

```
crates/
  api-gateway/
    src/
      main.rs              # Server startup, router
      state.rs             # AppState: DB pool, Redis, node registry, API keys
      auth.rs              # Key validation middleware
      cache.rs             # Redis rate limiter + generic TTL cache
      nodes.rs             # NodeInfo, NodeStatus types
      shard_coordinator.rs # Plan → execute pipeline
      db.rs                # PostgreSQL pool init + migrations
      routes/
        chat.rs            # POST /v1/chat/completions
        models.rs          # GET /v1/models
        nodes.rs           # Internal node registration
        keys.rs            # Internal key CRUD
    migrations/            # SQLx migrations (auto-applied on startup)

  node-agent/
    src/
      main.rs              # Startup, hardware detection, route definitions
      registration.rs      # Heartbeat loop with exponential backoff
      shard.rs             # /infer/shard handler + node chaining
      hardware.rs          # GPU detection via sysinfo

  shard-planner/
    src/
      lib.rs               # Public API: plan_shards, ModelRegistry, types
      planner.rs           # Greedy algorithm + unit tests
      registry.rs          # Model table + VRAM estimation

web/
  app/
    page.tsx               # Landing page
    status/page.tsx        # Network status dashboard
    api/nodes/route.ts     # Proxy: GET /v1/internal/nodes
    api/models/route.ts    # Proxy: GET /v1/models
  components/              # shadcn/ui + custom components
```

## Making Changes

### Adding a new model to the registry

Edit `crates/shard-planner/src/registry.rs` — add an entry to the `MODELS` table:

```rust
ModelSpec {
    name: "your-model:7b",
    total_layers: 32,
    vram_per_layer_mb: 150,
    context_vram_mb: 512,
},
```

Values are approximate. The `context_vram_mb` accounts for embeddings and KV cache (typically 256–1024 MB depending on context length and quantization).

### Adding a new API endpoint

1. Add handler in `crates/api-gateway/src/routes/`
2. Register route in `main.rs` router
3. Add to the appropriate auth layer (public key auth vs internal key)
4. Update API reference in `README.md`

### Changing the database schema

1. Add a new migration file in `crates/api-gateway/migrations/`
2. Name it `{N}_{description}.sql` where N is the next sequence number
3. SQLx runs migrations in order on gateway startup

## Code Style

- **No unnecessary comments.** Code should speak for itself. Only comment non-obvious invariants or workarounds.
- **Error handling:** use `anyhow` for internal errors, `thiserror` for public error types. Never `.unwrap()` in request handlers.
- **Logging:** use `tracing::info/warn/error`. Include structured fields (`tracing::info!(node_id = %id, "registered")`).
- **Environment variables:** always read them in `state.rs` or equivalent startup code, not scattered through handlers.

## Commit Messages

Follow conventional commits:

```
feat(api-gateway): add per-key daily spend cap enforcement
fix(node-agent): increase registration timeout to 30s
docs: add CONTRIBUTING guide
```

Add `Co-Authored-By: Paperclip <noreply@paperclip.ing>` on agent-generated commits.

## Deployment

See [`ARCHITECTURE.md`](./ARCHITECTURE.md) for full deployment context.

### API Gateway (bare metal / VM)

```bash
# Build release binary
cargo build --release --bin api-gateway

# Run with production env
DATABASE_URL=postgres://... \
REDIS_URL=redis://... \
INFER_INTERNAL_KEY=<long-random-secret> \
./target/release/api-gateway
```

### API Gateway (Docker)

```bash
docker build --target api-gateway -t infer-gateway:latest .
docker run -d \
  -p 8080:8080 \
  -e DATABASE_URL=postgres://... \
  -e REDIS_URL=redis://... \
  -e INFER_INTERNAL_KEY=... \
  infer-gateway:latest
```

### Node Agent (Docker)

```bash
docker build --target node-agent -t infer-agent:latest .
docker run -d \
  --network host \
  -e COORDINATOR_URL=http://<gateway>:8080 \
  -e INFER_INTERNAL_KEY=<same-as-gateway> \
  -e NODE_HOST=<this-machine-ip> \
  -e NODE_NAME=<unique-name> \
  infer-agent:latest
```

The node agent needs `--network host` (or a routable IP via `NODE_HOST`) so the gateway can reach the Ollama port and the agent port from outside the container.

### Web Dashboard (Vercel)

The `web/` directory is a Next.js app configured for Vercel deployment. Set these environment variables in your Vercel project:

| Variable | Value |
|---|---|
| `GATEWAY_URL` | Public or private URL of the API gateway |
| `GATEWAY_INTERNAL_KEY` | Internal key (mark as sensitive) |
| `GATEWAY_API_KEY` | A valid API key for `/v1/models` |

Set Root Directory to `web` in Vercel project settings (the repo root is a Rust workspace).
