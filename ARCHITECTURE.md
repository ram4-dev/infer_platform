# Architecture

## Resumen
Infer Platform expone una API OpenAI-compatible y enruta requests a nodos GPU que sirven un modelo concreto.

## Componentes
### API Gateway (`cmd/api-gateway`, `internal/gateway`)
Responsabilidades:
- autenticación por API key e internal key
- rate limiting Redis opcional
- registro de nodos
- routing `single_node_model`
- failover entre nodos del mismo modelo
- adaptación OpenAI-compatible de respuestas Ollama
- analytics, billing y jobs de background

### Node Agent (`cmd/node-agent`, `internal/nodeagent`)
Responsabilidades:
- descubrir hardware local
- registrar periódicamente el nodo en el gateway
- anunciar exactamente un modelo activo
- exponer endpoints de health/info/ping
- forward compatible con `/infer/shard`

### Web (`web/`)
Dashboard de estado, modelos, provider y claves.

## Flujo de inferencia
1. Cliente llama `POST /v1/chat/completions`.
2. Gateway valida API key y rate limit.
3. Gateway normaliza `model`.
4. Gateway busca nodos `online` que sirven exactamente ese modelo.
5. Ordena candidatos por latencia p50 y aplica round-robin por modelo.
6. En non-streaming hace failover secuencial si falla el nodo elegido.
7. En streaming usa el primer nodo seleccionado y proxea SSE.

## Persistencia
### PostgreSQL
- `nodes`
- `node_models`
- `node_health`
- `api_keys`
- `usage_logs`
- tablas de billing

### Redis
- contadores de rate limit por minuto

## Jobs de background
- stale node sweep
- health probing
- billing meter reporting
- provider payouts

## Contrato actual de nodo
Registro esperado:
- `model` obligatorio
- `license` obligatorio
- compatibilidad legacy con `models[]` de cardinalidad 1

## Despliegue
- `Dockerfile` compila y empaqueta los binarios Go
- `docker-compose.yml` levanta gateway, node-agent, postgres y redis
