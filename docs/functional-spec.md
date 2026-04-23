# Functional Specification — Migración de api-gateway y node-agent a Go

## Contexto
La plataforma expone una API OpenAI-compatible, registra nodos GPU remotos y enruta requests de inferencia al nodo adecuado. La RFC `RFC-001-single-node-model-routing.md` fija el comportamiento objetivo actual: **1 nodo = 1 modelo activo = 1 request completo**.

El objetivo de esta fase es consolidar las implementaciones de `api-gateway` y `node-agent` en Go, manteniendo la compatibilidad funcional del sistema.

## Objetivo
- Sustituir los binarios runtime principales por implementaciones en Go.
- Mantener los contratos HTTP existentes para clientes, dashboard y node-agents.
- Preservar el modo de routing `single_node_model`.
- Mantener PostgreSQL, Redis, Stripe y Ollama como integraciones externas.
- Consolidar PostgreSQL como única source of truth del gateway.

## Alcance
### Incluido
#### API Gateway
- `POST /v1/chat/completions`
- `GET /v1/models`
- `GET /v1/models/:id`
- `GET /health`
- `GET /ping`
- `POST /v1/internal/nodes`
- `GET /v1/internal/nodes`
- `POST /v1/internal/keys`
- `GET /v1/internal/keys`
- `DELETE /v1/internal/keys/:id`
- `GET /v1/internal/usage`
- `GET /v1/internal/licenses`
- `GET /v1/internal/provider/stats`
- `GET /v1/internal/analytics/consumer`
- `GET /v1/internal/models/stats`
- `POST /v1/billing/setup`
- `POST /v1/internal/billing/connect`
- `POST /v1/webhooks/stripe`
- Background jobs: stale node sweep, health probing, billing meter reporting, provider payouts.

#### Node Agent
- Startup y carga de configuración por variables de entorno.
- Registro periódico contra gateway.
- Endpoints `GET /health`, `GET /info`, `GET /ping`, `POST /infer/shard`.
- Recolección básica de hardware (GPU/VRAM/CPU/RAM).

## No alcance
- Reescritura del `infer` CLI.
- Reintroducir runtimes antiguos eliminados.
- Tensor parallelism real multi-nodo.
- Cambio de contratos del dashboard web.
- Cambio de esquema SQL existente.

## Requisitos funcionales
### Routing de inferencia
1. El gateway debe validar API key y rate limit antes de procesar inferencia.
2. El gateway debe normalizar el `model` a minúsculas y trim.
3. Debe seleccionar solo nodos `online` que anuncien exactamente ese modelo.
4. Debe ordenar candidatos por menor p50 y usar round-robin por modelo.
5. En non-streaming, debe aplicar failover secuencial entre nodos del mismo modelo.
6. En streaming, debe usar el primer candidato seleccionado por la misma política, sin reintentos mid-flight.
7. Si no hay nodos online del modelo, debe responder error claro `503`.
8. Nunca debe hacer fallback a otro modelo.

### Registro de nodos
1. El node-agent debe enviar únicamente `model` en el registro.
2. El gateway debe resolver internamente la `license` a partir de un catálogo `model -> license`.
3. Debe rechazar payloads inválidos con `422`.
4. Debe rechazar `license` externa y `models[]` legado.
5. Debe rechazar modelos desconocidos con `422`.
6. Debe garantizar un solo modelo activo por nodo en `node_models`.

### Modelos y dashboard
1. `/v1/models` debe listar modelos activos desde el registro de nodos online.
2. `/v1/models/:id` debe devolver metadata OpenAI-compatible y estadísticas agregadas.
3. El dashboard debe seguir pudiendo consultar nodos y modelos sin cambios de contrato.

### Billing y analytics
1. Deben mantenerse los endpoints de billing y analytics existentes.
2. Stripe debe seguir siendo opcional; si no está configurado, los endpoints dependientes deben degradar con error explícito.
3. Los cron jobs deben ser no bloqueantes y tolerar fallos transitorios.

## Requisitos no funcionales
- Binarios estáticos y simples de desplegar.
- Timeout explícito para integraciones HTTP salientes.
- Fail-open para rate limiting si Redis no está disponible.
- Logs estructurados legibles.
- Compatibilidad con configuración actual vía `.env`/env vars.
- `DATABASE_URL` obligatorio para arrancar el gateway.
- Si la DB falla durante un request, no debe existir fallback persistente en memoria.
- El streaming debe usar lectura buffered line-by-line, sin `bufio.Scanner`.

## Estado actual implementado
- Gateway en modo DB-only.
- Bun introducido como capa de acceso a datos del runtime Go.
- Registro de nodos model-only con resolución interna de licencia.
- Streaming endurecido con `bufio.Reader`.
- Analytics y billing migrados a repositorios Bun.

## Estrategia de migración
### Etapa 1
- Introducir implementación Go con tests unitarios básicos para routing y validación de registro.

### Etapa 2
- Cambiar Docker/compose para ejecutar binarios Go por defecto.
- Eliminar código legado y referencias operativas antiguas.

### Etapa 3
- Validación end-to-end con PostgreSQL, Redis, Ollama y dashboard.

## Criterios de aceptación
1. `go test ./...` pasa para los nuevos binarios Go.
2. `docker-compose` puede levantar gateway y node-agent Go.
3. El dashboard web sigue funcionando contra los mismos endpoints.
4. Chat streaming y non-streaming funcionan con selección single-node-model.
5. Registro de nodos con `NODE_MODEL` funciona sin `NODE_MODEL_LICENSE`.
6. El gateway sigue siendo compatible con PostgreSQL, Redis y Stripe opcional.
7. El gateway no arranca sin `DATABASE_URL`.
8. El gateway resuelve la licencia internamente y persiste `node_models.license`.
9. Los endpoints críticos no usan memoria como fallback persistente.
