#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
GATEWAY_PORT="${GATEWAY_PORT:-18080}"
AGENT_PORT="${AGENT_PORT:-18181}"
OLLAMA_BASE_URL="${OLLAMA_BASE_URL:-http://127.0.0.1:11434}"
OLLAMA_HOST="${OLLAMA_HOST:-127.0.0.1}"
OLLAMA_PORT="${OLLAMA_PORT:-11434}"
MODEL="${MODEL:-qwen2.5:0.5b}"
INTERNAL_KEY="${INTERNAL_KEY:-internal_dev_secret}"
DATABASE_URL="${DATABASE_URL:-postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable}"

cleanup() {
  jobs -pr | xargs -r kill >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 1; }
}

need_cmd go
need_cmd curl
need_cmd python3

cd "$ROOT_DIR"

echo "==> Checking Ollama availability at ${OLLAMA_BASE_URL}"
curl -fsS "${OLLAMA_BASE_URL}/api/tags" >/dev/null

echo "==> Ensuring model is available: ${MODEL}"
curl -fsS "${OLLAMA_BASE_URL}/api/pull" \
  -H 'Content-Type: application/json' \
  -d "$(python3 - <<PY
import json
print(json.dumps({"name": "${MODEL}", "stream": False}))
PY
)" >/dev/null

echo "==> Building binaries"
go build -o "$TMP_DIR/api-gateway" ./cmd/api-gateway
go build -o "$TMP_DIR/node-agent" ./cmd/node-agent

echo "==> Verifying gateway requires DATABASE_URL"
if PORT="$GATEWAY_PORT" INFER_INTERNAL_KEY="$INTERNAL_KEY" ROUTING_MODE=single_node_model "$TMP_DIR/api-gateway" >"$TMP_DIR/gateway-no-db.log" 2>&1; then
  echo "Gateway unexpectedly started without DATABASE_URL" >&2
  exit 1
fi

echo "==> Starting gateway"
PORT="$GATEWAY_PORT" \
DATABASE_URL="$DATABASE_URL" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
ROUTING_MODE=single_node_model \
"$TMP_DIR/api-gateway" >"$TMP_DIR/gateway.log" 2>&1 &

for _ in $(seq 1 40); do
  if curl -fsS "http://127.0.0.1:${GATEWAY_PORT}/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
curl -fsS "http://127.0.0.1:${GATEWAY_PORT}/health" >/dev/null

echo "==> Creating API key"
API_KEY="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" -H 'Content-Type: application/json' \
  -d '{"owner":"e2e-real-ollama","rate_limit_rpm":120}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/keys" | python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])')"

echo "==> Starting node-agent against real Ollama"
COORDINATOR_URL="http://127.0.0.1:${GATEWAY_PORT}" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
NODE_NAME="real-ollama-node" NODE_HOST="127.0.0.1" NODE_PORT="$OLLAMA_PORT" AGENT_PORT="$AGENT_PORT" \
NODE_MODEL="$MODEL" \
"$TMP_DIR/node-agent" >"$TMP_DIR/node-agent.log" 2>&1 &

for _ in $(seq 1 60); do
  body="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/nodes" || true)"
  if python3 - "$body" "$MODEL" <<'PY'
import json, sys
body = json.loads(sys.argv[1])
model = sys.argv[2]
items = body.get("data", [])
ok = any(item.get("model") == model and item.get("status") == "online" for item in items)
sys.exit(0 if ok else 1)
PY
  then
    break
  fi
  sleep 1
done

NODES_JSON="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/nodes")"
MODELS_JSON="$(curl -fsS -H "Authorization: Bearer ${API_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/models")"

PROMPT="Say exactly: E2E_REAL_OK"
NON_STREAM_FILE="$TMP_DIR/non-stream.json"
curl -fsS -H "Authorization: Bearer ${API_KEY}" -H 'Content-Type: application/json' \
  -d "$(python3 - <<PY
import json
print(json.dumps({
  "model": "${MODEL}",
  "messages": [{"role": "user", "content": "${PROMPT}"}]
}))
PY
)" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions" >"$NON_STREAM_FILE"

STREAM_FILE="$TMP_DIR/stream.out"
curl -sS -N -H "Authorization: Bearer ${API_KEY}" -H 'Content-Type: application/json' \
  -d "$(python3 - <<PY
import json
print(json.dumps({
  "model": "${MODEL}",
  "stream": True,
  "messages": [{"role": "user", "content": "${PROMPT}"}]
}))
PY
)" \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions" >"$STREAM_FILE"

python3 - "$NODES_JSON" "$MODELS_JSON" "$NON_STREAM_FILE" "$STREAM_FILE" "$MODEL" <<'PY'
import json, pathlib, sys
nodes = json.loads(sys.argv[1])
models = json.loads(sys.argv[2])
non_stream = json.loads(pathlib.Path(sys.argv[3]).read_text())
stream_out = pathlib.Path(sys.argv[4]).read_text()
model = sys.argv[5]

assert nodes["total"] >= 1, nodes
assert any(item.get("model") == model for item in nodes["data"]), nodes
assert any(item.get("id") == model for item in models["data"]), models
content = non_stream["choices"][0]["message"]["content"]
assert content.strip() != "", non_stream
assert "[DONE]" in stream_out, stream_out
assert "chat.completion.chunk" in stream_out, stream_out
print("Non-stream content:", content[:200])
print("Real Ollama E2E assertions passed")
PY

echo "=== REAL OLLAMA E2E SUCCESS ==="
echo "Model: $MODEL"
echo "Nodes: $NODES_JSON"
echo "Models: $MODELS_JSON"
echo "Non-stream response:"
cat "$NON_STREAM_FILE"
echo
echo "Stream sample:"
head -n 10 "$STREAM_FILE"
