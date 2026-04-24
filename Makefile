.PHONY: help docker-up docker-ps docker-down docker-logs cli-build cli-commands infer-client-test dev-up

DOCKER_COMPOSE ?= docker-compose
CLI_ENTRY := node infer-cli/dist/src/cli.js
TS_COMPILER := node web/node_modules/typescript/bin/tsc
GATEWAY_URL ?= http://127.0.0.1:8080
OLLAMA_URL ?= http://127.0.0.1:11434
MODEL ?= qwen2.5:0.5b
INTERNAL_KEY ?= internal_dev_secret
PROMPT ?= Hola desde el cliente, responde con una frase corta.

help:
	@echo "Targets disponibles:"
	@echo "  make docker-up      # levanta postgres, redis y api-gateway"
	@echo "  make docker-ps      # muestra el estado de los containers"
	@echo "  make docker-down    # baja los containers"
	@echo "  make docker-logs    # sigue logs del api-gateway"
	@echo "  make cli-build      # compila el CLI infer"
	@echo "  make cli-commands   # imprime los comandos del CLI a ejecutar"
	@echo "  make infer-client-test # crea una API key y prueba inferencia como cliente"
	@echo "  make dev-up         # levanta Docker y luego imprime los comandos del CLI"

docker-up:
	@echo "==> Levantando Docker para usar el CLI (postgres, redis, api-gateway)..."
	@$(DOCKER_COMPOSE) up -d postgres redis api-gateway
	@echo
	@echo "==> Docker levantado:"
	@$(DOCKER_COMPOSE) ps postgres redis api-gateway
	@echo
	@$(MAKE) --no-print-directory cli-commands

docker-ps:
	@$(DOCKER_COMPOSE) ps postgres redis api-gateway

docker-down:
	@echo "==> Bajando containers..."
	@$(DOCKER_COMPOSE) down

docker-logs:
	@$(DOCKER_COMPOSE) logs -f api-gateway

cli-build:
	@echo "==> Compilando CLI infer..."
	@$(TS_COMPILER) -p infer-cli/tsconfig.json
	@echo "CLI compilado en infer-cli/dist"

cli-commands:
	@echo "==> Comandos del CLI para usar el entorno levantado"
	@echo
	@echo "1) Compilar el CLI:"
	@echo "   $(TS_COMPILER) -p infer-cli/tsconfig.json"
	@echo
	@echo "2) Verificar prerequisitos:"
	@echo "   $(CLI_ENTRY) doctor $(MODEL) --gateway $(GATEWAY_URL) --ollama-url $(OLLAMA_URL)"
	@echo
	@echo "3) Arrancar el node-agent gestionado por el CLI en detached:"
	@echo "   $(CLI_ENTRY) run $(MODEL) --gateway $(GATEWAY_URL) --token $(INTERNAL_KEY) --detach"
	@echo
	@echo "4) Consultar estado:"
	@echo "   $(CLI_ENTRY) status"
	@echo
	@echo "5) Probar inferencia desde el lado del cliente:"
	@echo "   make infer-client-test MODEL=$(MODEL)"
	@echo
	@echo "6) Ver logs:"
	@echo "   $(CLI_ENTRY) logs --follow"
	@echo
	@echo "7) Detener el agente:"
	@echo "   $(CLI_ENTRY) stop"
	@echo
	@echo "Notas:"
	@echo "- El CLI MVP usa Ollama local en $(OLLAMA_URL)."
	@echo "- El token del ejemplo es la INFER_INTERNAL_KEY del gateway: $(INTERNAL_KEY)."
	@echo "- Este flujo NO levanta el service 'node-agent' por Docker, porque el CLI lo gestiona localmente."

infer-client-test:
	@echo "==> Creando API key de cliente temporal y probando inferencia..."
	@API_KEY="$$(curl -fsS \
	  -H "Authorization: Bearer $(INTERNAL_KEY)" \
	  -H 'Content-Type: application/json' \
	  -d '{"owner":"make-client-test","rate_limit_rpm":120}' \
	  "$(GATEWAY_URL)/v1/internal/keys" | \
	  python3 -c 'import json,sys; print(json.load(sys.stdin)["key"])')"; \
	echo "API key creada: $$API_KEY"; \
	echo; \
	echo "==> Ejecutando POST /v1/chat/completions con model=$(MODEL)"; \
	curl -fsS \
	  -H "Authorization: Bearer $$API_KEY" \
	  -H 'Content-Type: application/json' \
	  -d '{"model":"$(MODEL)","messages":[{"role":"user","content":"$(PROMPT)"}]}' \
	  "$(GATEWAY_URL)/v1/chat/completions"; \
	echo

dev-up: docker-up
