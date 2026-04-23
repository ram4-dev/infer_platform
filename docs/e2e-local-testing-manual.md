# Manual de testeo E2E en local

## Objetivo
Este documento describe cómo validar end-to-end en local el runtime actual de Infer Platform:
- `api-gateway` en Go
- `node-agent` en Go
- PostgreSQL como source of truth obligatoria del gateway
- Redis opcional
- backend de inferencia simulado o real

Incluye dos recorridos:
1. **E2E automatizado** con `scripts/e2e_go_runtime.sh`
2. **E2E manual** con `docker-compose`, `curl` y validaciones funcionales

---

## 1. Prerrequisitos

### Software requerido
- Go 1.26+
- Python 3
- `curl`
- PostgreSQL accesible localmente
- Docker / Docker Compose (para el recorrido con contenedores)

### Puertos usados habitualmente
- Gateway: `8080` o `18080`
- Node Agent: `8181`
- PostgreSQL: `5432`
- Redis: `6379`
- Mock Ollama: `19000`

### Variables importantes
#### Gateway
- `DATABASE_URL` (**obligatoria**)
- `INFER_INTERNAL_KEY`
- `PORT`
- `REDIS_URL` (opcional)
- `ROUTING_MODE=single_node_model`

#### Node Agent
- `COORDINATOR_URL`
- `INFER_INTERNAL_KEY`
- `NODE_NAME`
- `NODE_HOST`
- `NODE_PORT`
- `AGENT_PORT`
- `NODE_MODEL`

---

## 2. Opción A — Test E2E automatizado recomendado

El camino más fiable para verificar el runtime actual es ejecutar:

```bash
DATABASE_URL=postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable \
bash scripts/e2e_go_runtime.sh
```

## Qué valida este script
- que el gateway arranca con DB
- que se puede crear una API key interna
- que el node-agent registra nodos con payload **model-only**
- que `/v1/internal/nodes` lista 3 nodos
- que `/v1/models` devuelve los modelos online esperados
- que el chat non-stream hace failover y responde correctamente
- que un modelo desconocido devuelve `503`
- que el streaming devuelve SSE con `[DONE]`

## Resultado esperado
El script debe terminar con una salida similar a:

```text
E2E assertions passed
=== E2E SUCCESS ===
```

## Si falla
Revisar:
- conectividad a PostgreSQL
- disponibilidad del puerto `5432`
- disponibilidad de Python 3
- logs temporales generados por el script

---

## 3. Opción B — Test E2E manual con docker-compose

Este recorrido sirve para validar arranque del stack y contratos básicos.

### 3.1 Levantar servicios base

```bash
docker compose up --build postgres redis api-gateway node-agent
```

### 3.2 Verificar salud del gateway

```bash
curl -fsS http://127.0.0.1:8080/health | jq
```

Resultado esperado:
- HTTP `200`
- campo `status: "ok"`

### 3.3 Verificar health del node-agent

```bash
curl -fsS http://127.0.0.1:8181/health | jq
```

Resultado esperado:
- HTTP `200`
- `registered: true` tras el primer ciclo de registro

### 3.4 Listar nodos registrados

```bash
curl -fsS \
  -H 'Authorization: Bearer internal_dev_secret' \
  http://127.0.0.1:8080/v1/internal/nodes | jq
```

Resultado esperado:
- `total >= 1`
- cada nodo con `model`
- `license` resuelta por el gateway

### 3.5 Crear API key de prueba

```bash
curl -fsS \
  -H 'Authorization: Bearer internal_dev_secret' \
  -H 'Content-Type: application/json' \
  -d '{"owner":"local-e2e","rate_limit_rpm":120}' \
  http://127.0.0.1:8080/v1/internal/keys | jq
```

Guardar el valor `key` devuelto.

### 3.6 Listar modelos públicos

```bash
export API_KEY='pk_xxx'

curl -fsS \
  -H "Authorization: Bearer ${API_KEY}" \
  http://127.0.0.1:8080/v1/models | jq
```

Resultado esperado:
- lista de modelos online
- ids de modelo disponibles

---

## 4. Opción C — E2E manual completo con backend mock

Este flujo replica la idea del script automatizado, pero manualmente.

### 4.1 Arrancar PostgreSQL
Si no lo tienes ya levantado:

```bash
docker compose up -d postgres redis
```

### 4.2 Compilar binarios

```bash
go build -o /tmp/api-gateway ./cmd/api-gateway
go build -o /tmp/node-agent ./cmd/node-agent
```

### 4.3 Levantar un mock de Ollama
Crear `/tmp/mock_ollama.py` con este contenido:

```python
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def _set_headers(self, status=200, content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.end_headers()

    def log_message(self, format, *args):
        return

    def do_POST(self):
        if self.path != "/api/chat":
            self._set_headers(404)
            self.wfile.write(b'{}')
            return
        length = int(self.headers.get("Content-Length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        messages = body.get("messages", [])
        prompt = messages[-1]["content"] if messages else ""
        if body.get("stream"):
            self._set_headers(200, "application/x-ndjson")
            chunks = [
                {"message": {"content": "mock:"}, "done": False},
                {"message": {"content": prompt}, "done": False},
                {"done": True, "prompt_eval_count": 3, "eval_count": 2},
            ]
            for chunk in chunks:
                self.wfile.write(json.dumps(chunk).encode() + b"\n")
                self.wfile.flush()
            return
        self._set_headers()
        self.wfile.write(json.dumps({
            "message": {"content": f"mock:{prompt}"},
            "prompt_eval_count": 3,
            "eval_count": 2
        }).encode())

HTTPServer(("127.0.0.1", 19000), Handler).serve_forever()
```

Ejecutar:

```bash
python3 /tmp/mock_ollama.py
```

### 4.4 Arrancar gateway
En otra terminal:

```bash
PORT=18080 \
DATABASE_URL=postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable \
INFER_INTERNAL_KEY=internal_dev_secret \
ROUTING_MODE=single_node_model \
/tmp/api-gateway
```

### 4.5 Crear una API key

```bash
API_KEY="$(curl -fsS \
  -H 'Authorization: Bearer internal_dev_secret' \
  -H 'Content-Type: application/json' \
  -d '{"owner":"e2e-manual","rate_limit_rpm":120}' \
  http://127.0.0.1:18080/v1/internal/keys | python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])')"

echo "$API_KEY"
```

### 4.6 Arrancar nodes de prueba

#### Nodo malo para failover
```bash
COORDINATOR_URL=http://127.0.0.1:18080 \
INFER_INTERNAL_KEY=internal_dev_secret \
NODE_NAME=bad-failover-node \
NODE_HOST=127.0.0.1 \
NODE_PORT=19001 \
AGENT_PORT=18182 \
NODE_MODEL=failover-model \
/tmp/node-agent
```

#### Nodo bueno para failover
```bash
COORDINATOR_URL=http://127.0.0.1:18080 \
INFER_INTERNAL_KEY=internal_dev_secret \
NODE_NAME=good-failover-node \
NODE_HOST=127.0.0.1 \
NODE_PORT=19000 \
AGENT_PORT=18183 \
NODE_MODEL=failover-model \
/tmp/node-agent
```

#### Nodo para streaming
```bash
COORDINATOR_URL=http://127.0.0.1:18080 \
INFER_INTERNAL_KEY=internal_dev_secret \
NODE_NAME=stream-node \
NODE_HOST=127.0.0.1 \
NODE_PORT=19000 \
AGENT_PORT=18181 \
NODE_MODEL=stream-model \
/tmp/node-agent
```

---

## 5. Casos de prueba manuales

### Caso 1 — Listado de nodos internos

```bash
curl -fsS \
  -H 'Authorization: Bearer internal_dev_secret' \
  http://127.0.0.1:18080/v1/internal/nodes | jq
```

Esperado:
- `total == 3`
- cada nodo con `model`
- `license` inferida por catálogo

### Caso 2 — Listado de modelos públicos

```bash
curl -fsS \
  -H "Authorization: Bearer ${API_KEY}" \
  http://127.0.0.1:18080/v1/models | jq
```

Esperado:
- aparecen `failover-model` y `stream-model`

### Caso 3 — Non-stream con failover

```bash
curl -fsS \
  -H "Authorization: Bearer ${API_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{"model":"failover-model","messages":[{"role":"user","content":"hello failover"}]}' \
  http://127.0.0.1:18080/v1/chat/completions | jq
```

Esperado:
- respuesta `200`
- contenido `mock:hello failover`
- el primer nodo falla y el segundo resuelve

### Caso 4 — Modelo desconocido

```bash
curl -i \
  -H "Authorization: Bearer ${API_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{"model":"missing-model","messages":[{"role":"user","content":"hello"}]}' \
  http://127.0.0.1:18080/v1/chat/completions
```

Esperado:
- HTTP `503`
- mensaje indicando que el modelo no está disponible

### Caso 5 — Streaming SSE

```bash
curl -N \
  -H "Authorization: Bearer ${API_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{"model":"stream-model","stream":true,"messages":[{"role":"user","content":"hello stream"}]}' \
  http://127.0.0.1:18080/v1/chat/completions
```

Esperado:
- chunks SSE `data: {...}`
- aparece el texto `hello stream`
- finaliza con `data: [DONE]`

### Caso 6 — Contrato de registro model-only
Intentar registrar un nodo enviando `license` externa debe fallar con `422`.

Ejemplo:

```bash
curl -i \
  -H 'Authorization: Bearer internal_dev_secret' \
  -H 'Content-Type: application/json' \
  -d '{
    "name":"bad-node",
    "host":"127.0.0.1",
    "port":11434,
    "agent_port":8181,
    "gpu_name":"gpu",
    "vram_mb":8192,
    "model":"llama3.1:8b",
    "license":"llama-3.1"
  }' \
  http://127.0.0.1:18080/v1/internal/nodes
```

Esperado:
- HTTP `422`

---

## 6. Qué revisar en caso de fallo

### Gateway no arranca
Comprobar:
- `DATABASE_URL`
- acceso real a PostgreSQL
- puerto libre
- logs de arranque

### Node agent no registra
Comprobar:
- `COORDINATOR_URL`
- `INFER_INTERNAL_KEY`
- `NODE_MODEL`
- que el modelo exista en el catálogo del gateway

### `/v1/models` está vacío
Comprobar:
- que los nodos estén `online`
- que el registro se haya completado
- que el modelo figure en `node_models`

### Streaming falla
Comprobar:
- backend NDJSON
- que el mock o Ollama real devuelva líneas terminadas en `\n`
- que no haya proxies intermedios cortando SSE

---

## 7. Checklist de validación final

- [ ] Gateway arranca con `DATABASE_URL`
- [ ] Node agent registra usando solo `NODE_MODEL`
- [ ] `/v1/internal/nodes` lista nodos online
- [ ] `/v1/models` expone modelos activos
- [ ] chat non-stream responde correctamente
- [ ] failover entre nodos del mismo modelo funciona
- [ ] modelo desconocido devuelve `503`
- [ ] streaming SSE termina con `[DONE]`
- [ ] registro con `license` externa falla con `422`
- [ ] `go test ./...` pasa

---

## 8. Comandos rápidos

### Ejecutar tests unitarios
```bash
go test ./...
```

### Ejecutar E2E automatizado
```bash
DATABASE_URL=postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable \
bash scripts/e2e_go_runtime.sh
```

### Levantar DB y Redis
```bash
docker compose up -d postgres redis
```
