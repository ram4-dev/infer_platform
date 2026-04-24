# Instructivo — uso del CLI `infer` interno (MVP)

## Objetivo
Este instructivo explica cómo compilar y usar el CLI `infer` interno para testers de confianza.

El MVP actual permite:
- validar prerequisitos locales,
- comprobar Ollama,
- iniciar `node-agent`,
- guardar estado local,
- consultar estado,
- ver logs,
- detener el agente.

## Alcance de este MVP
Este CLI está pensado para:
- uso interno,
- alfa testers de confianza,
- Ollama local,
- registro usando `INFER_INTERNAL_KEY`.

### Limitaciones actuales
- no publica en npm,
- no descarga binarios desde releases,
- no valida checksums de artefactos remotos,
- no soporta onboarding público de hosts,
- no soporta Windows en este instructivo,
- asume Ollama local en `localhost` o `127.0.0.1`.

---

## Ubicación del CLI
El código vive en:

```text
infer-cli/
```

Archivos principales:
- `infer-cli/src/cli.ts`
- `infer-cli/src/commands/`
- `infer-cli/src/lib/`

---

## Requisitos
- Node.js 20+
- Go 1.26+
- PostgreSQL accesible si vas a usar gateway local
- Ollama corriendo en local

### Recomendado para pruebas locales
- gateway en `http://127.0.0.1:8080` o `http://127.0.0.1:18080`
- Ollama en `http://127.0.0.1:11434`

---

## Estado local que crea el CLI
El CLI guarda su estado en:

```text
~/.infer/
├─ bin/
├─ config.json
├─ state.json
└─ logs/
   └─ node-agent.log
```

### Archivos importantes
- `~/.infer/config.json`: configuración persistente
- `~/.infer/state.json`: estado operacional actual
- `~/.infer/logs/node-agent.log`: log del `node-agent`

---

## Cómo compilar el CLI
Desde la raíz del repo:

```bash
node web/node_modules/typescript/bin/tsc -p infer-cli/tsconfig.json
```

O con Make:

```bash
make cli-build
```

Esto genera los archivos compilados en:

```text
infer-cli/dist/
```

---

## Cómo ver la ayuda
```bash
node infer-cli/dist/src/cli.js help
```

---

## Preparación del entorno
Antes de correr `infer run`, asegúrate de tener:

1. Ollama corriendo
2. un gateway accesible
3. una `INFER_INTERNAL_KEY` válida

### Levantar Docker para usar el CLI
Puedes levantar la infraestructura necesaria con:

```bash
make docker-up
```

Eso levanta:
- `postgres`
- `redis`
- `api-gateway`

Y deja fuera `node-agent`, porque en este MVP el `node-agent` lo gestiona el CLI local.

Para ver el estado de los containers:

```bash
make docker-ps
```

Para bajar todo:

```bash
make docker-down
```

### Verificar Ollama
```bash
curl -fsS http://127.0.0.1:11434/api/tags | jq
```

### Verificar gateway
```bash
curl -fsS http://127.0.0.1:8080/health | jq
```

---

## Comando `doctor`
Sirve para validar prerequisitos antes de arrancar el agente.

### Ejemplo
```bash
node infer-cli/dist/src/cli.js doctor qwen2.5:0.5b \
  --gateway http://127.0.0.1:8080 \
  --ollama-url http://127.0.0.1:11434
```

### Qué valida
- escritura en `~/.infer`
- reachability de Ollama
- presencia del modelo en Ollama si se indica
- reachability del gateway
- disponibilidad del binario `node-agent`

### Resultado esperado
Verás checks tipo:

```text
✓ Writable directory: /Users/tu-usuario/.infer
✓ Ollama reachable: http://127.0.0.1:11434
✓ Gateway reachable: http://127.0.0.1:8080
✓ node-agent binary available
```

---

## Comando `run`
Arranca el `node-agent` gestionado por el CLI.

### Flags principales
- `--gateway <url>`
- `--token <internal-key>`
- `--ollama-url <url>`
- `--node-name <name>`
- `--host <ip>`
- `--agent-port <port>`
- `--ollama-port <port>`
- `--detach`
- `--foreground`
- `--no-pull`
- `--agent-bin <path>`

### Importante
- `--token` debe ser la `INFER_INTERNAL_KEY` del gateway para este MVP.
- si no hay binario disponible de `node-agent`, el CLI intentará compilarlo localmente con:

```bash
go build -o ... ./cmd/node-agent
```

### Ejemplo en foreground
```bash
node infer-cli/dist/src/cli.js run qwen2.5:0.5b \
  --gateway http://127.0.0.1:8080 \
  --token internal_dev_secret \
  --foreground
```

### Ejemplo en detached
```bash
node infer-cli/dist/src/cli.js run qwen2.5:0.5b \
  --gateway http://127.0.0.1:8080 \
  --token internal_dev_secret \
  --detach
```

### Qué hace internamente
1. valida config y prerequisitos
2. verifica Ollama
3. verifica si el modelo existe
4. si falta y no usaste `--no-pull`, intenta hacer pull en Ollama
5. resuelve el binario de `node-agent`
6. si hace falta, compila el binario local
7. escribe config y state en `~/.infer`
8. arranca el `node-agent`
9. verifica el health local del agente
10. intenta verificar registro remoto en el gateway

### Resultado esperado
Salida parecida a:

```text
✓ Writable directory: /Users/tu-usuario/.infer
✓ Ollama reachable: http://127.0.0.1:11434
✓ Model present in Ollama: qwen2.5:0.5b
✓ Gateway reachable: http://127.0.0.1:8080
✓ node-agent binary available
✓ node-agent started in detached mode (PID 12345)
✓ Local health: ok registered=true
✓ Remote registration verified
```

---

## Comando `status`
Muestra el estado local conocido por el CLI.

### Ejemplo
```bash
node infer-cli/dist/src/cli.js status
```

### Qué muestra
- si está corriendo o no
- PID
- modelo
- gateway
- URL de Ollama
- binario en uso
- ruta de logs
- health local si responde
- verificación remota si se puede hacer

---

## Comando `logs`
Muestra los logs del `node-agent` gestionado por el CLI.

### Ver logs actuales
```bash
node infer-cli/dist/src/cli.js logs
```

### Seguir logs
```bash
node infer-cli/dist/src/cli.js logs --follow
```

---

## Comando `stop`
Detiene el proceso gestionado por el CLI.

### Ejemplo
```bash
node infer-cli/dist/src/cli.js stop
```

### Qué hace
- lee el PID desde `~/.infer/state.json`
- envía `SIGTERM`
- actualiza el estado local

---

## Ejemplo completo de uso

### 1. Levantar Docker para el gateway
```bash
make docker-up
```

### 2. Compilar el CLI
```bash
make cli-build
```

### 3. Verificar prerequisitos
```bash
node infer-cli/dist/src/cli.js doctor qwen2.5:0.5b \
  --gateway http://127.0.0.1:8080 \
  --ollama-url http://127.0.0.1:11434
```

### 4. Arrancar el agente
```bash
node infer-cli/dist/src/cli.js run qwen2.5:0.5b \
  --gateway http://127.0.0.1:8080 \
  --token internal_dev_secret \
  --detach
```

### 5. Consultar estado
```bash
node infer-cli/dist/src/cli.js status
```

### 6. Seguir logs
```bash
node infer-cli/dist/src/cli.js logs --follow
```

### 7. Detener el agente
```bash
node infer-cli/dist/src/cli.js stop
```

### 8. Bajar Docker
```bash
make docker-down
```

---

## Troubleshooting

### Error: missing `--gateway`
Debes pasar `--gateway` en `infer run` o tenerlo persistido previamente en `~/.infer/config.json`.

### Error: missing `--token`
En este MVP el token es la `INFER_INTERNAL_KEY` del gateway.

### Error: Ollama check failed
Verifica que Ollama esté corriendo en local y que responda en:

```bash
curl -fsS http://127.0.0.1:11434/api/tags | jq
```

### Error: model missing in Ollama
Puedes dejar que el CLI haga pull automáticamente o descargarlo antes.

### Error al compilar `node-agent`
Verifica:
- que `go` esté instalado,
- que el repo esté íntegro,
- que `go build ./cmd/node-agent` funcione manualmente.

### Remote registration not yet verified
Puede pasar si el gateway tarda unos segundos en reflejar el registro. Revisa:
- `infer status`
- `infer logs`
- `GET /v1/internal/nodes`

---

## Relación con otros instructivos
Para pruebas operativas relacionadas, ver también:
- `docs/instructions/e2e-local-testing-manual.md`
- `docs/instructions/e2e-real-ollama-step-by-step.md`
