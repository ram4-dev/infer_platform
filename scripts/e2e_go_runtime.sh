#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
GATEWAY_PORT=18080
GOOD_OLLAMA_PORT=19000
BAD_OLLAMA_PORT=19001
STREAM_AGENT_PORT=18181
BAD_FAILOVER_AGENT_PORT=18182
GOOD_FAILOVER_AGENT_PORT=18183
INTERNAL_KEY="internal_dev_secret"
DATABASE_URL="${DATABASE_URL:-postgres://infer:infer_dev_password@127.0.0.1:5432/infer?sslmode=disable}"

cleanup() {
  jobs -pr | xargs -r kill >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

cat >"$TMP_DIR/mock_ollama.py" <<'PY'
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def _set_headers(self, status=200, content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.end_headers()

    def log_message(self, format, *args):
        return

    def do_GET(self):
        if self.path == "/api/tags":
            self._set_headers()
            self.wfile.write(json.dumps({"models": [{"name": "stream-model", "modified_at": "", "size": 1}, {"name": "failover-model", "modified_at": "", "size": 1}]}).encode())
            return
        self._set_headers(404)
        self.wfile.write(b'{}')

    def do_POST(self):
        if self.path != "/api/chat":
            self._set_headers(404)
            self.wfile.write(b'{}')
            return
        length = int(self.headers.get("Content-Length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        messages = body.get("messages", [])
        prompt = messages[-1]["content"] if messages else ""
        content = f"mock:{prompt}"
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
        self.wfile.write(json.dumps({"message": {"content": content}, "prompt_eval_count": 3, "eval_count": 2}).encode())

HTTPServer(("127.0.0.1", 19000), Handler).serve_forever()
PY

cd "$ROOT_DIR"

go build -o "$TMP_DIR/api-gateway" ./cmd/api-gateway
go build -o "$TMP_DIR/node-agent" ./cmd/node-agent

python3 "$TMP_DIR/mock_ollama.py" >"$TMP_DIR/mock-ollama.log" 2>&1 &

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

API_KEY="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" -H 'Content-Type: application/json' \
  -d '{"owner":"e2e","rate_limit_rpm":120}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/keys" | python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])')"

COORDINATOR_URL="http://127.0.0.1:${GATEWAY_PORT}" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
NODE_NAME="bad-failover-node" NODE_HOST="127.0.0.1" NODE_PORT="$BAD_OLLAMA_PORT" AGENT_PORT="$BAD_FAILOVER_AGENT_PORT" \
NODE_MODEL="failover-model" \
"$TMP_DIR/node-agent" >"$TMP_DIR/bad-failover-agent.log" 2>&1 &

sleep 1

COORDINATOR_URL="http://127.0.0.1:${GATEWAY_PORT}" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
NODE_NAME="good-failover-node" NODE_HOST="127.0.0.1" NODE_PORT="$GOOD_OLLAMA_PORT" AGENT_PORT="$GOOD_FAILOVER_AGENT_PORT" \
NODE_MODEL="failover-model" \
"$TMP_DIR/node-agent" >"$TMP_DIR/good-failover-agent.log" 2>&1 &

COORDINATOR_URL="http://127.0.0.1:${GATEWAY_PORT}" \
INFER_INTERNAL_KEY="$INTERNAL_KEY" \
NODE_NAME="stream-node" NODE_HOST="127.0.0.1" NODE_PORT="$GOOD_OLLAMA_PORT" AGENT_PORT="$STREAM_AGENT_PORT" \
NODE_MODEL="stream-model" \
"$TMP_DIR/node-agent" >"$TMP_DIR/stream-agent.log" 2>&1 &

for _ in $(seq 1 40); do
  body="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/nodes" || true)"
  if python3 - "$body" <<'PY'
import json, sys
body = json.loads(sys.argv[1])
sys.exit(0 if body.get("total") == 3 else 1)
PY
  then
    break
  fi
  sleep 0.5
done

NODES_JSON="$(curl -fsS -H "Authorization: Bearer ${INTERNAL_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/internal/nodes")"
MODELS_JSON="$(curl -fsS -H "Authorization: Bearer ${API_KEY}" "http://127.0.0.1:${GATEWAY_PORT}/v1/models")"
FAILOVER_JSON="$(curl -fsS -H "Authorization: Bearer ${API_KEY}" -H 'Content-Type: application/json' \
  -d '{"model":"failover-model","messages":[{"role":"user","content":"hello failover"}]}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions")"

UNKNOWN_RESPONSE_FILE="$TMP_DIR/unknown.out"
UNKNOWN_STATUS="$(curl -sS -o "$UNKNOWN_RESPONSE_FILE" -w '%{http_code}' -H "Authorization: Bearer ${API_KEY}" -H 'Content-Type: application/json' \
  -d '{"model":"missing-model","messages":[{"role":"user","content":"hello"}]}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions")"

STREAM_OUT="$TMP_DIR/stream.out"
curl -sS -N -H "Authorization: Bearer ${API_KEY}" -H 'Content-Type: application/json' \
  -d '{"model":"stream-model","stream":true,"messages":[{"role":"user","content":"hello stream"}]}' \
  "http://127.0.0.1:${GATEWAY_PORT}/v1/chat/completions" >"$STREAM_OUT"

python3 - "$NODES_JSON" "$MODELS_JSON" "$FAILOVER_JSON" "$UNKNOWN_STATUS" "$UNKNOWN_RESPONSE_FILE" "$STREAM_OUT" <<'PY'
import json, pathlib, sys
nodes = json.loads(sys.argv[1])
models = json.loads(sys.argv[2])
failover = json.loads(sys.argv[3])
unknown_status = sys.argv[4]
unknown_body = pathlib.Path(sys.argv[5]).read_text()
stream_out = pathlib.Path(sys.argv[6]).read_text()

assert nodes["total"] == 3, nodes
model_ids = sorted(item["id"] for item in models["data"])
assert model_ids == ["failover-model", "stream-model"], model_ids
assert failover["choices"][0]["message"]["content"] == "mock:hello failover", failover
assert unknown_status == "503", unknown_status
assert "missing-model" in unknown_body, unknown_body
assert "[DONE]" in stream_out, stream_out
assert "hello stream" in stream_out, stream_out
print("E2E assertions passed")
PY

echo "=== E2E SUCCESS ==="
echo "Nodes: $NODES_JSON"
echo "Models: $MODELS_JSON"
echo "Failover response: $FAILOVER_JSON"
echo "Unknown model status: $UNKNOWN_STATUS"
echo "Stream sample:"
head -n 5 "$STREAM_OUT"
