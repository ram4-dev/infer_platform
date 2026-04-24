# Infer Platform

Infer Platform es una plataforma de inferencia distribuida con API OpenAI-compatible.

## Stack actual
- **api-gateway:** Go
- **node-agent:** Go
- **web:** Next.js / TypeScript
- **DB obligatoria para gateway:** PostgreSQL
- **Rate limiting opcional:** Redis
- **Backend de inferencia:** Ollama

## Modelo operativo actual
La plataforma sigue el modo definido en `docs/RFC-001-single-node-model-routing.md`:
- **1 nodo = 1 modelo activo**
- **1 request = 1 nodo**
- balanceo y failover solo entre nodos del mismo modelo

## Estructura del repo
```text
cmd/
  api-gateway/
  node-agent/
internal/
  gateway/
  nodeagent/
web/
docs/
Dockerfile
docker-compose.yml
```

## Ejecutar local
### Requisitos
- Go 1.26+
- Node.js 20+
- Ollama
- PostgreSQL obligatorio para el gateway
- Redis opcional

### Gateway
```bash
DATABASE_URL=postgres://infer:infer_dev_password@localhost:5432/infer?sslmode=disable \
go run ./cmd/api-gateway
```

### Node agent
```bash
NODE_MODEL=llama3.1:8b \
go run ./cmd/node-agent
```

### Web
```bash
cd web
npm install
npm run dev
```

## Variables relevantes
### Gateway
- `PORT`
- `INFER_INTERNAL_KEY`
- `OLLAMA_URL`
- `DATABASE_URL` (obligatoria)
- `REDIS_URL`
- `ROUTING_MODE=single_node_model`
- Stripe env vars opcionales

### Node agent
- `NODE_NAME`
- `NODE_HOST`
- `NODE_PORT`
- `AGENT_PORT`
- `COORDINATOR_URL`
- `INFER_INTERNAL_KEY`
- `NODE_MODEL`
- `GPU_NAME`, `GPU_VRAM_MB` opcionales

## Comandos útiles
```bash
go test ./...
go build ./cmd/api-gateway ./cmd/node-agent
docker-compose up --build
```

## Estado actual del runtime
- Gateway DB-only: sin fallback persistente en memoria.
- Acceso a datos Bun por dominios: nodes, keys, usage, stats y billing.
- Registro de nodos model-only: el gateway resuelve la licencia internamente.
- Streaming endurecido con `bufio.Reader` para NDJSON/SSE.

## Documentación
- `docs/RFC-001-single-node-model-routing.md`
- `docs/functional-spec.md`
- `docs/technical-spec.md`
- `docs/implementation-plan-bun.md`

## Instructivos
- `docs/instructions/README.md`
- `docs/instructions/cli-internal-mvp.md`
- `docs/instructions/e2e-local-testing-manual.md`
- `docs/instructions/e2e-real-ollama-step-by-step.md`
