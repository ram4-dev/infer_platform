# Implementation Plan — Bun migration and runtime hardening

## Status
Implemented in the current Go runtime:
- **Bun** adopted as the ORM / data-access layer for runtime domains.
- Gateway runs in **DB-only** mode.
- Node registration is **model-only**.
- Streaming was hardened with buffered reads.

Documentation updated in:
- `docs/functional-spec.md`
- `docs/technical-spec.md`
- `README.md`

## Goals
Apply the following fixes in order:
1. Replace `bufio.Scanner` with buffered streaming reads.
2. Replace ad-hoc SQL calls with **Bun**.
3. Node registration sends **only `model`**; the gateway resolves the license internally.
4. Remove in-memory fallback as a source of truth.
   - PostgreSQL becomes the **only SoT**.
   - If DB is not configured or unavailable, the gateway must fail fast on startup or return `503` where appropriate.

---

## Architectural decisions

### 1. ORM choice
- ORM / DB layer: **Bun**
- Driver base: PostgreSQL via Bun-supported stack
- Strategy: incremental migration by domain, not a full rewrite in one pass

### 2. Source of truth
- PostgreSQL is the **only** source of truth for:
  - nodes
  - node_models
  - api_keys
  - usage_logs
  - billing data
  - health-derived persistent data
- In-memory state may remain only for ephemeral runtime concerns:
  - round-robin offsets by model
  - short-lived derived latency caches
  - process-local counters/metrics
- In-memory storage must **not** be used as fallback persistence.

### 3. Registration contract
- `node-agent` will send:
  - `name`
  - `host`
  - `port`
  - `agent_port`
  - `gpu_name`
  - `vram_mb`
  - `model`
- `license` stops being part of the node-agent registration payload.
- The gateway resolves the license from an internal catalog:
  - `model -> license`
- Unknown model => `422`

### 4. Streaming implementation
- Replace `bufio.Scanner` with buffered line reads using `bufio.Reader`.
- Goal:
  - avoid token-size limits
  - handle larger Ollama NDJSON chunks safely

---

## Work phases

## Phase 1 — DB-only runtime
### Objective
Remove all runtime fallback paths that treat in-memory state as persistence.

### Changes
- `DATABASE_URL` becomes mandatory for `api-gateway`.
- `NewApp()` fails if DB connection cannot be established.
- Remove write-path fallback to `NodeStore`.
- Remove read-path fallback to `NodeStore` for business state.
- Keep only ephemeral in-memory state where justified.

### Files affected
- `internal/gateway/config.go`
- `internal/gateway/types.go`
- `internal/gateway/server.go`
- `internal/gateway/chat.go`
- `internal/gateway/nodes.go`
- `internal/gateway/stats.go`
- `internal/gateway/billing.go`
- `internal/gateway/jobs.go`
- `internal/gateway/store.go` (shrink or repurpose)

### Acceptance criteria
- Gateway does not start without `DATABASE_URL`.
- If DB calls fail in request flow, endpoints return `503`/server error rather than using in-memory fallback.
- No persistence write-path falls back to memory.

---

## Phase 2 — Introduce Bun data layer
### Objective
Replace direct SQL calls with Bun-backed repository/data access.

### Proposed structure
- `internal/gateway/db/`
  - `models.go`
  - `bun.go`
  - `repo_nodes.go`
  - `repo_keys.go`
  - `repo_usage.go`
  - `repo_stats.go`
  - `repo_billing.go`

### Bun models
At minimum:
- `Node`
- `NodeModel`
- `NodeHealth`
- `APIKey`
- `UsageLog`
- `BillingCustomer`
- `ProviderStripeAccount`
- `BillingMeterReport`

### Migration order
1. Nodes / node_models
2. API keys / auth lookups
3. Usage / analytics / models stats / provider stats
4. Billing / webhook writes / cron jobs

### Acceptance criteria
- No plain SQL in the request/runtime path.
- Multi-step DB updates use transactions.
- Repository boundaries exist and are reused by handlers/jobs.

---

## Phase 3 — Model catalog and license resolution
### Objective
Make the gateway the only place that resolves model license policy.

### Changes
- Add internal catalog, e.g. `internal/gateway/model_catalog.go`
- Provide:
  - normalized model matching
  - `ResolveLicense(model) (license, ok)`
- Registration validation changes:
  - require `model`
  - reject external `license`
  - reject unknown models with `422`
- Persist resolved license into `node_models.license`

### Files affected
- `internal/gateway/types.go`
- `internal/gateway/nodes.go`
- `internal/nodeagent/nodeagent.go`
- `cmd/node-agent/.env.example`
- docs

### Acceptance criteria
- Node agent sends only `model`.
- Gateway resolves and stores license.
- Unknown model returns `422`.
- No node-provided license is trusted.

---

## Phase 4 — Buffered streaming reads
### Objective
Harden SSE/NDJSON streaming against large chunks.

### Changes
- Replace `bufio.Scanner` in `internal/gateway/chat.go`
- Use `bufio.Reader` + line-oriented reads (`ReadBytes('\n')` or equivalent)
- Preserve OpenAI-compatible SSE output

### Acceptance criteria
- Streaming still works in E2E.
- Large NDJSON lines do not fail because of scanner token limits.
- `[DONE]` behavior remains unchanged.

---

## Phase 5 — Tests and validation
### Unit tests
Add or update tests for:
- DB-required startup behavior
- model catalog lookup and license resolution
- registration with model-only payload
- unknown model rejection
- buffered stream chunk parsing
- Bun repositories and transactional replacement of `node_models`

### E2E
Update `scripts/e2e_go_runtime.sh` to validate:
- gateway requires DB or fails fast
- node-agent registers with `model` only
- gateway resolves license
- non-stream failover still works
- streaming still works with buffered reader

### Manual smoke checks
- `/v1/internal/nodes`
- `/v1/models`
- `/v1/chat/completions` stream + non-stream
- registration of known/unknown model

---

## Known risks
1. Bun migration touches many files at once
2. analytics/provider stats may need schema-aligned cleanup during migration
3. removing memory fallback may change local developer workflow
4. model catalog ownership must be kept up to date

## Mitigations
- Migrate one domain at a time
- Keep contracts stable while moving internals
- Update docs/examples/compose in the same change set
- Add tests before replacing critical paths

---

## Recommended implementation order
1. DB-only runtime
2. Bun for nodes / node_models / auth-critical paths
3. model -> license catalog
4. buffered streaming
5. Bun for stats and billing
6. E2E update and docs cleanup

---

## Definition of done
- Gateway is DB-only and Bun-backed.
- Node registration is model-only.
- License resolution is centralized in the gateway.
- No in-memory persistence fallback remains.
- Streaming no longer uses `bufio.Scanner`.
- Unit tests pass.
- E2E script is updated for DB-only startup and model-only registration.
