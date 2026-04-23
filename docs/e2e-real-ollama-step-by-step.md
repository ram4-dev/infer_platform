# E2E real con Ollama paso a paso

## Objetivo
Esta guía documenta cómo correr una prueba end-to-end **real** de Infer Platform usando:
- `api-gateway` en Go
- `node-agent` en Go
- PostgreSQL local
- Ollama local
- un modelo pequeño real
- inferencia real vía `POST /v1/chat/completions`

El flujo validado es:
1. levantar Ollama
2. descargar un modelo chico
3. levantar el gateway
4. crear una API key
5. levantar un node-agent conectado a Ollama
6. verificar que el nodo quedó registrado
7. listar modelos públicos
8. correr inferencia real non-stream y stream

---

## Modelo recomendado
Para una prueba rápida usar:

```text
qwen2.5:0.5b
```

Es suficientemente chico para una validación local.

---

## Requisitos
- Go `1.26+`
- PostgreSQL accesible en local
- Ollama instalado y corriendo
- `curl`
- `jq`
- `python3`

---

## Variables usadas en esta guía
```bash
export DATABASE_URL='postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable'
export INTERNAL_KEY='internal_dev_secret'
export GATEWAY_PORT='18080'
export AGENT_PORT='18181'
export MODEL='qwen2.5:0.5b'
```

---

## 1. Verificar PostgreSQL
Confirmar que PostgreSQL responde en `127.0.0.1:5432`.

Ejemplo:
```bash
lsof -nP -iTCP:5432 -sTCP:LISTEN
```

Si no está levantado, iniciar tu instancia local antes de seguir.

---

## 2. Verificar Ollama
Comprobar que Ollama está corriendo:

```bash
curl -fsS http://127.0.0.1:11434/api/tags | jq
```

Si responde JSON, Ollama está disponible.

---

## 3. Descargar un modelo chico real
Descargar `qwen2.5:0.5b` en Ollama:

```bash
curl -fsS \
  http://127.0.0.1:11434/api/pull \
  -H 'Content-Type: application/json' \
  -d '{"name":"qwen2.5:0.5b","stream":false}'
```

### Nota importante sobre zsh/bash
Si escribes el comando en varias líneas, la barra invertida `\` debe ser el último carácter de la línea.

Esto es **incorrecto** si hay espacios después de `\`:

```bash
curl -fsS http://127.0.0.1:11434/api/pull \ 
  -H 'Content-Type: application/json'
```

Eso puede producir errores como:
- `405`
- `zsh: command not found: -H`
- `zsh: command not found: -d`

Si querés ver progreso de descarga, usar streaming:

```bash
curl \
  http://127.0.0.1:11434/api/pull \
  -H 'Content-Type: application/json' \
  -d '{"name":"qwen2.5:0.5b","stream":true}'
```

Verificar luego que el modelo existe:

```bash
curl -fsS http://127.0.0.1:11434/api/tags | jq '.models | map(.name)'
```

Esperado:
```json
[
  "qwen2.5:0.5b"
]
```

---

## 4. Compilar los binarios
Desde la raíz del repo:

```bash
go build -o /tmp/api-gateway ./cmd/api-gateway
go build -o /tmp/node-agent ./cmd/node-agent
```

---

## 5. Levantar el gateway
En una terminal nueva:

```bash
PORT="$GATEWAY_PORT" \
DATABASE_URL="$DATABASE_URL" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
ROUTING_MODE=single_node_model \
/tmp/api-gateway
```

Verificar salud:

```bash
curl -fsS "http://127.0.0.1:${GATEWAY_PORT}/health" | jq
```

Esperado:
- HTTP `200`
- `status: "ok"`

Ejemplo:

```json
{
  "status": "ok",
  "service": "infer-api-gateway"
}
```

---

## 6. Crear una API key de cliente
En otra terminal:

```bash
export API_KEY="$(curl -fsS \
  -H "Authorization: Bearer ${INTERNAL_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{"owner":"manual-real-e2e","rate_limit_rpm":120}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/keys" | \
  python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])')"

printf '%s\n' "$API_KEY"
```

Esperado:
- se imprime una key tipo `pk_...`

---

## 7. Levantar el node-agent conectado a Ollama real
En otra terminal:

```bash
COORDINATOR_URL="http://127.0.0.1:${GATEWAY_PORT}" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
NODE_NAME='real-local-node' \
NODE_HOST='127.0.0.1' \
NODE_PORT='11434' \
AGENT_PORT="$AGENT_PORT" \
NODE_MODEL="$MODEL" \
/tmp/node-agent
```

### Qué hace este proceso
- expone el health del agent
- se registra periódicamente en el gateway
- anuncia que sirve el modelo configurado en `NODE_MODEL`
- enruta inferencia real hacia Ollama en `NODE_HOST:NODE_PORT`

---

## 8. Verificar health del node-agent
```bash
curl -fsS "http://127.0.0.1:${AGENT_PORT}/health" | jq
```

Esperado:
- `status: "ok"`
- `registered: true`

Ejemplo:

```json
{
  "status": "ok",
  "node_name": "real-local-node",
  "registered": true
}
```

---

## 9. Verificar que el nodo quedó registrado en la plataforma
```bash
curl -fsS \
  -H "Authorization: Bearer ${INTERNAL_KEY}" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/nodes" | jq
```

Esperado:
- `total >= 1`
- aparece el nodo `real-local-node`
- `status: "online"`
- `model: "qwen2.5:0.5b"`

Ejemplo:

```json
{
  "data": [
    {
      "name": "real-local-node",
      "status": "online",
      "model": "qwen2.5:0.5b",
      "license": "apache-2.0"
    }
  ],
  "total": 1
}
```

---

## 10. Verificar que el modelo es visible para clientes
```bash
curl -fsS \
  -H "Authorization: Bearer ${API_KEY}" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/models" | jq
```

Esperado:
- aparece `qwen2.5:0.5b`

Ejemplo:

```json
{
  "data": [
    {
      "id": "qwen2.5:0.5b",
      "object": "model",
      "owned_by": "infer-platform"
    }
  ]
}
```

---

## 11. Probar inferencia real non-stream
```bash
curl -fsS \
  -H "Authorization: Bearer ${API_KEY}" \
  -H 'Content-Type: application/json' \
  -d "$(python3 - <<PY
import json
print(json.dumps({
  'model': '${MODEL}',
  'messages': [
    {'role': 'user', 'content': 'Respondé con una sola línea: REAL_E2E_OK'}
  ]
}))
PY
)" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions" | jq
```

Esperado:
- HTTP `200`
- respuesta OpenAI-compatible
- `choices[0].message.content` con texto real generado por el modelo

Ejemplo:

```json
{
  "object": "chat.completion",
  "model": "qwen2.5:0.5b",
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "REAL_E2E_OK"
      }
    }
  ]
}
```

> Nota: como la inferencia es real, el modelo puede no responder exactamente la frase pedida. Lo importante es que devuelva contenido válido y no vacío.

---

## 12. Probar inferencia real stream
```bash
curl -N \
  -H "Authorization: Bearer ${API_KEY}" \
  -H 'Content-Type: application/json' \
  -d "$(python3 - <<PY
import json
print(json.dumps({
  'model': '${MODEL}',
  'stream': True,
  'messages': [
    {'role': 'user', 'content': 'Contá 1 2 3 en texto corto'}
  ]
}))
PY
)" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions"
```

Esperado:
- eventos SSE `data: {...}`
- chunks con `chat.completion.chunk`
- final con `data: [DONE]`

---

## 13. Qué valida esta prueba
Esta prueba confirma que:
- Ollama sirve inferencia real
- el `node-agent` se registra correctamente
- el `api-gateway` publica el modelo registrado
- un cliente con API key puede pedir inferencia
- el tráfico fluye por la plataforma completa
- streaming y non-stream funcionan

---

## 14. Troubleshooting rápido

### Error `405` en `/api/pull`
Suele pasar si el comando `curl` quedó cortado por un problema de multilinea.

Usar exactamente:

```bash
curl -fsS \
  http://127.0.0.1:11434/api/pull \
  -H 'Content-Type: application/json' \
  -d '{"name":"qwen2.5:0.5b","stream":false}'
```

### `zsh: command not found: -H`
Hay espacios después de `\` o la línea quedó partida incorrectamente.

### `/v1/models` vacío
Comprobar:
- que el `node-agent` siga vivo
- que `/health` del agent devuelva `registered: true`
- que `/v1/internal/nodes` muestre el nodo `online`

### `/v1/chat/completions` devuelve `503`
Comprobar:
- que el `model` pedido coincide exactamente con `NODE_MODEL`
- que Ollama tenga descargado el modelo
- que el nodo siga online

### El modelo no responde exactamente lo pedido
Es normal en inferencia real. Para este E2E alcanza con validar que:
- hay respuesta
- el contenido no está vacío
- el modelo correcto fue enrutado

---

## 15. Comando automatizado equivalente
Si querés validar el mismo flujo con el script del repo:

```bash
DATABASE_URL='postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable' \
MODEL='qwen2.5:0.5b' \
bash scripts/e2e_go_runtime_real_ollama.sh
```

---

## 16. Checklist final
- [ ] Ollama responde en `127.0.0.1:11434`
- [ ] `qwen2.5:0.5b` está descargado
- [ ] gateway arriba y sano
- [ ] API key creada
- [ ] node-agent arriba y registrado
- [ ] `/v1/internal/nodes` muestra el nodo online
- [ ] `/v1/models` expone `qwen2.5:0.5b`
- [ ] non-stream responde con contenido real
- [ ] stream emite chunks SSE y termina en `[DONE]`
