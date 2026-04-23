# Technical Specification — Migración de api-gateway y node-agent a Go

## Decisión técnica
Se implementarán dos nuevos binarios en Go:
- `cmd/api-gateway`
- `cmd/node-agent`

El runtime del sistema queda en Go y Docker/compose apuntan únicamente a los binarios Go.

## Estructura propuesta
- `go.mod`
- `cmd/api-gateway/main.go`
- `cmd/node-agent/main.go`
- `internal/common/` helpers compartidos mínimos
- `internal/gateway/` lógica HTTP, auth, routing, DB, billing, health jobs
- `internal/nodeagent/` configuración, hardware, registro, shard forward

## Stack Go
- Router HTTP: `github.com/go-chi/chi/v5`
- ORM / data access: **Bun**
- PostgreSQL: Bun-backed PostgreSQL connection layer
- Redis: `github.com/redis/go-redis/v9`
- UUID: `github.com/google/uuid`
- Dotenv: `github.com/joho/godotenv`
- Logging: stdlib `log/slog`

## API Gateway
### Componentes
#### Config
Carga env vars actuales:
- `PORT`
- `INFER_INTERNAL_KEY`
- `OLLAMA_URL`
- `DATABASE_URL` (**obligatorio**)
- `REDIS_URL`
- `ROUTING_MODE`
- Stripe env vars existentes

#### Estado global
- Config
- conexión Bun / PostgreSQL obligatoria
- Cliente Redis opcional
- HTTP client compartido
- repositorios Bun por dominio (`nodes`, `keys`, `usage`, `stats`, `billing`)
- caches in-memory sólo para estado efímero derivado
- Round-robin offsets por modelo
- Contadores por modelo (`requests`, `failovers`, `no_capacity`)

#### Middleware auth
- Bearer parser
- API key validation desde DB únicamente
- Inserción de `ValidatedKey` en contexto
- Rate limiting Redis con `INCR` + `EXPIRE`
- Middleware separado para internal key
- Sin fallback a API keys en memoria

#### Model router
Entrada:
- modelo normalizado
- lista de nodos candidatos
- mapa de latencias

Salida:
- lista ordenada de `NodeInfo`

Reglas:
- filtrar `online`
- filtrar `node.model == model`
- ordenar por `p50_ms ASC`, luego `vram_mb DESC`
- aplicar round-robin por modelo rotando el slice

#### Chat completions
- Sanitización de request
- Selección de candidatos desde DB únicamente
- Streaming:
  - enviar request a Ollama del nodo seleccionado
  - transformar chunks Ollama a SSE OpenAI-compatible
  - leer chunks NDJSON con buffered reader, no con `bufio.Scanner`
- Non-streaming:
  - reintentos entre candidatos del mismo modelo
  - transformar respuesta Ollama a JSON OpenAI-compatible
- Registro de uso en `usage_logs`

#### Node registry
- Validación del contrato model-only
- Normalización `model`
- Resolución de `license` desde catálogo interno `model -> license`
- Upsert en `nodes`
- Reemplazo transaccional del set de `node_models` para dejar 1 registro por nodo
- `GET /v1/internal/nodes` con `model/license`

#### Models/health/stats
- `/v1/models` desde `node_models JOIN nodes`
- `/v1/models/:id` con agregados DB y cache efímera de salud
- `/health` con conteo de nodos online
- analytics/provider stats vía repositorios Bun

#### Billing
- Cliente Stripe con `application/x-www-form-urlencoded`
- Webhook signature validation con HMAC SHA256
- Endpoints setup/connect/webhook equivalentes
- Persistencia y lookups vía repositorio Bun de billing
- Cron jobs:
  - hourly meter reporting
  - nightly payouts

#### Background jobs
- stale sweep cada 30s
- health probing cada 30s
- meter reporter cada 1h
- payout checker cada 5m

## Node Agent
### Config
Carga:
- `NODE_NAME`
- `NODE_HOST`
- `NODE_PORT`
- `AGENT_PORT`
- `COORDINATOR_URL`
- `INFER_INTERNAL_KEY`
- `NODE_MODEL`
- `NODE_MODEL_LICENSE` (deprecated; dejará de ser enviado por el agent)
- `GPU_NAME`, `GPU_VRAM_MB` opcionales

### Hardware detection
- Overrides por env
- Linux: lectura de `/proc/driver/nvidia/gpus/*/information`
- macOS: memoria unificada aproximada
- fallback `Unknown GPU`

### Registro periódico
- POST cada 30s a `/v1/internal/nodes`
- backoff exponencial en fallo
- payload con `model` únicamente; la licencia la resuelve el gateway
- `NODE_MODEL_LICENSE` deja de formar parte del contrato operativo

### Endpoints
- `/health`: estado del agente
- `/info`: metadata del nodo/modelo
- `/ping`: liveness
- `/infer/shard`: compatibilidad con el endpoint histórico; ejecuta inferencia local vía Ollama y reenvía al siguiente shard si existiera

## Docker
### Dockerfile
Multi-stage build con Go para ambos binarios.

### docker-compose
Servicios `api-gateway` y `node-agent` deberán construir desde el Dockerfile Go.
- `api-gateway` requiere `DATABASE_URL`
- `node-agent` requiere `NODE_MODEL`
- `NODE_MODEL_LICENSE` ya no es necesaria

## Testing
### Unit tests mínimos obligatorios
- selección de candidatos por modelo y estado
- round-robin por modelo
- validación de registro single-model
- hashing / bearer parsing básicos si aplica

### Smoke manual esperado
- levantar PostgreSQL + Redis + gateway + node-agent
- registrar nodo
- listar nodos/modelos
- invocar chat non-stream
- invocar chat stream

## Riesgos técnicos
- Diferencias sutiles en SSE respecto a la implementación previa
- Diferencias en tipos `NULL`/aggregates al portar queries a Go
- Complejidad del billing si se prueba sin Stripe real

## Mitigaciones
- Mantener contratos JSON idénticos donde sea posible
- Probar consultas críticas con tests y smoke checks
- Hacer billing opcional y defensivo

## Estado implementado en el repo
- `internal/gateway/db/bun.go`: apertura de conexión Bun/PG.
- `internal/gateway/db/models.go`: modelos Bun base.
- `internal/gateway/db/repo_nodes.go`: nodos, modelos, health y probe targets.
- `internal/gateway/db/repo_keys.go`: API keys y auth lookups.
- `internal/gateway/db/repo_usage.go`: inserción y agregados de uso.
- `internal/gateway/db/repo_stats.go`: analytics, infer stats, models stats y provider stats.
- `internal/gateway/db/repo_billing.go`: billing customers, connect accounts, meter reports.
- `internal/gateway/model_catalog.go`: catálogo `model -> license`.
- `internal/gateway/chat.go`: streaming con `bufio.Reader`.
- `internal/nodeagent/nodeagent.go`: registro model-only.
