# Runtime Change Summary — Bun / DB-only hardening

## Purpose
This document records the runtime changes applied from `docs/implementation-plan-bun.md` so they remain explicitly declared in the repository.

## Applied changes

### 1. Gateway is DB-only
- `DATABASE_URL` is mandatory.
- `NewApp()` fails fast if DB connection cannot be established.
- In-memory persistence is no longer used as fallback source of truth.
- Request flows now return server error / `503` instead of silently falling back to memory.

### 2. Bun repositories introduced
Repository layer under `internal/gateway/db/`:
- `bun.go`
- `models.go`
- `repo_nodes.go`
- `repo_keys.go`
- `repo_usage.go`
- `repo_stats.go`
- `repo_billing.go`

### 3. Node registration contract changed
`node-agent` sends only:
- `name`
- `host`
- `port`
- `agent_port`
- `gpu_name`
- `vram_mb`
- `model`

The gateway:
- resolves `license` internally from a model catalog
- rejects external `license`
- rejects legacy `models[]`
- rejects unknown models with `422`

### 4. Model catalog centralized in gateway
- `internal/gateway/model_catalog.go`
- normalized lookup
- `ResolveLicense(model)`
- persisted license stored in `node_models.license`

### 5. Streaming hardened
- `internal/gateway/chat.go`
- `bufio.Scanner` replaced by `bufio.Reader`
- line-based reads using buffered streaming
- OpenAI-compatible SSE preserved
- `[DONE]` behavior preserved

### 6. Analytics and billing migrated to Bun repos
- Stats, analytics and provider views now read through repository boundaries.
- Billing setup/connect/webhook/meter reporting/payout flows now use Bun-backed repositories.

## Files updated
- `README.md`
- `docs/functional-spec.md`
- `docs/technical-spec.md`
- `docs/implementation-plan-bun.md`
- `internal/gateway/config.go`
- `internal/gateway/types.go`
- `internal/gateway/server.go`
- `internal/gateway/chat.go`
- `internal/gateway/nodes.go`
- `internal/gateway/stats.go`
- `internal/gateway/billing.go`
- `internal/gateway/jobs.go`
- `internal/gateway/model_catalog.go`
- `internal/gateway/db/*`
- `internal/nodeagent/nodeagent.go`
- `cmd/api-gateway/.env.example`
- `cmd/node-agent/.env.example`
- `docker-compose.yml`
- `scripts/e2e_go_runtime.sh`

## Validation recorded
- `go test ./...` passes
- E2E script updated to reflect DB-only startup and model-only registration

## Engram note
No direct Engram integration/tooling was available in this session. This file can be used as the canonical summary to copy into Engram if needed.
