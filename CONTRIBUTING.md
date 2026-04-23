# Contributing

## Requisitos
- Go 1.26+
- Node.js 20+
- Docker + docker-compose
- [Ollama](https://ollama.ai) corriendo localmente con al menos un modelo descargado

## Desarrollo local
### 1. Levantar dependencias opcionales
```bash
docker-compose up postgres redis -d
```

### 2. Ejecutar el API gateway
```bash
cd cmd/api-gateway
cp .env.example .env
# ajusta DATABASE_URL / REDIS_URL si quieres modo persistente
cd ../..
go run ./cmd/api-gateway
```

### 3. Ejecutar un node-agent
```bash
cd cmd/node-agent
cp .env.example .env
cd ../..
NODE_MODEL=llama3.1:8b \
NODE_MODEL_LICENSE=llama-3.1 \
go run ./cmd/node-agent
```

### 4. Ejecutar el dashboard web
```bash
cd web
cat > .env.local <<'EOF'
GATEWAY_URL=http://localhost:8080
GATEWAY_INTERNAL_KEY=internal_dev_secret
GATEWAY_API_KEY=pk_your_key_here
EOF
npm install
npm run dev
```

## Testing
### Go
```bash
go test ./...
go build ./cmd/api-gateway ./cmd/node-agent
```

### E2E runtime
```bash
./scripts/e2e_go_runtime.sh
```

### Web
```bash
cd web
npm run build
```

## Estructura del proyecto
```text
cmd/
  api-gateway/
  node-agent/
internal/
  gateway/
  nodeagent/
web/
docs/
scripts/
Dockerfile
docker-compose.yml
```

## Cambios habituales
### Añadir un endpoint nuevo
1. Añade handler en `internal/gateway/`
2. Registra la ruta en `internal/gateway/server.go`
3. Aplica el middleware correcto: API key o internal key
4. Actualiza `README.md` si cambia la API pública

### Cambiar esquema DB
El esquema se aplica desde `internal/gateway/migrations.go`.
- añade la sentencia SQL nueva al listado de migrations
- mantén compatibilidad con instalaciones existentes
- prueba el arranque del gateway con `DATABASE_URL`

### Cambiar routing
- mantén el modo `single_node_model`
- no introduzcas fallback entre modelos
- conserva la misma política para streaming y non-streaming

## Estilo
- comentarios solo cuando expliquen invariantes o decisiones no obvias
- errores explícitos y respuestas JSON consistentes
- logs estructurados y sobrios
- centraliza env vars en el arranque del servicio

## Commits
Convención recomendada:
```text
feat(gateway): add per-model metric
fix(node-agent): retry registration on timeout
docs: update contributing guide
```

## Deploy
### Docker
```bash
docker build --target api-gateway -t infer-gateway:latest .
docker build --target node-agent -t infer-agent:latest .
```

### Compose
```bash
docker-compose up --build
```

## Referencias
- `README.md`
- `ARCHITECTURE.md`
- `docs/RFC-001-single-node-model-routing.md`
- `docs/functional-spec.md`
- `docs/technical-spec.md`
