# Technical Specification — Infer CLI npm + descarga de binario Go

## Estado
Propuesta. No implementado.

## Decisión técnica
Se propone una arquitectura híbrida:
- **CLI `infer` en Node.js**, distribuida vía npm/npx.
- **`node-agent` existente en Go**, distribuido como binario precompilado por plataforma.
- **Gateway actual en Go**, sin cambios de stack por esta iniciativa.

La CLI actúa como capa de:
- distribución
- onboarding
- configuración local
- orquestación de procesos
- diagnóstico

El `node-agent` Go sigue siendo el runtime de larga vida que habla con el gateway y con Ollama.

## Motivación técnica
### Por qué Node/npm para la CLI
- Mejor UX de distribución con `npx`.
- Menor fricción para pruebas rápidas y actualizaciones.
- Ecosistema sólido para CLI, prompts y output amigable.
- La CLI es principalmente orchestration/HTTP/process management, no runtime crítico de inferencia.

### Por qué mantener `node-agent` en Go
- Ya existe en el repo.
- Evita reescribir la lógica operacional del agente.
- Es adecuado como proceso persistente de bajo consumo y larga vida.
- Mantiene continuidad con el runtime actual.

## Arquitectura propuesta
```text
npm package: infer
├─ CLI Node.js
│  ├─ command parsing
│  ├─ platform detection
│  ├─ artifact resolution/download
│  ├─ local config/state management
│  ├─ Ollama checks
│  ├─ gateway checks
│  └─ process lifecycle management
│
└─ local machine
   ├─ ~/.infer/bin/node-agent
   ├─ ~/.infer/config.json
   ├─ ~/.infer/state.json
   └─ ~/.infer/logs/node-agent.log

runtime services
├─ local Ollama
├─ downloaded node-agent (Go)
└─ remote or local Infer gateway
```

## Componentes de la CLI Node
### 1. Command layer
Responsable de parsear y ejecutar:
- `run`
- `status`
- `stop`
- `doctor`
- `logs`

Librerías posibles:
- `commander` o `oclif`
- `chalk` para salida
- `ora` para spinners
- `prompts` o `enquirer` para modo interactivo

### 2. Platform resolver
Responsable de mapear:
- `process.platform`
- `process.arch`

A un target de artefacto, por ejemplo:
- `darwin-arm64`
- `darwin-amd64`
- `linux-arm64`
- `linux-amd64`
- `windows-amd64`

### 3. Artifact resolver/downloader
Responsable de:
- resolver URL de descarga según versión y plataforma
- descargar el binario del `node-agent`
- verificar checksum/firma si está disponible
- descomprimir si el artefacto viene en `.tar.gz` o `.zip`
- marcar permisos de ejecución en Unix
- cachear por versión

### 4. Local config/state store
Archivos locales propuestos:
- `~/.infer/config.json`
- `~/.infer/state.json`
- `~/.infer/logs/node-agent.log`
- `~/.infer/bin/<version>/<platform>/node-agent`
- `~/.infer/bin/current/node-agent` o referencia equivalente

`config.json` debe guardar configuración persistente del host.
`state.json` debe guardar estado efímero operacional.

### 5. Ollama integration layer
Responsable de:
- chequear `GET /api/tags`
- inspeccionar si el modelo existe localmente
- disparar pull con `POST /api/pull`
- esperar finalización o stream de progreso

### 6. Process manager
Responsable de:
- arrancar el `node-agent`
- redirigir stdout/stderr a log
- guardar pid
- detectar exit codes tempranos
- permitir parada controlada

En primera fase no instala servicios del sistema. Solo maneja proceso local.

### 7. Gateway integration layer
Responsable de:
- validar reachability del gateway
- ejecutar, si existe, un flujo de enrolamiento
- opcionalmente verificar estado remoto post-registro

## Distribución de artefactos Go
### Publicación
Se propone publicar binarios del `node-agent` por plataforma en releases versionados, por ejemplo en GitHub Releases.

Artefactos esperados por versión:
- `node-agent-darwin-arm64.tar.gz`
- `node-agent-darwin-amd64.tar.gz`
- `node-agent-linux-arm64.tar.gz`
- `node-agent-linux-amd64.tar.gz`
- `node-agent-windows-amd64.zip`
- `checksums.txt`

### Resolución de versión
La CLI debe definir una estrategia explícita:
- por defecto usa la versión del paquete npm actual
- opcionalmente permite `--agent-version`
- `npx infer@latest` alinea CLI y runtime a la última versión publicada

### Integridad
Validación mínima recomendada:
- checksum SHA256 contra `checksums.txt`

Validación futura opcional:
- firma del manifiesto
- provenance attestation

## Flujo técnico de `infer run`
```text
1. parse args
2. resolve local paths
3. validate write permissions
4. detect OS/arch
5. check gateway URL syntax
6. check Ollama reachability
7. query local models
8. if missing model -> pull model
9. resolve agent artifact version/platform
10. download and verify artifact if absent
11. write local config/state
12. spawn node-agent with derived env vars
13. wait for local /health
14. optionally verify remote registration
15. persist final running state and print success
```

## Variables y configuración derivada
La CLI no debería requerir que el usuario exporte manualmente todas las env vars del `node-agent`. Debe derivarlas internamente.

Variables a pasar al `node-agent`:
- `NODE_NAME`
- `NODE_HOST`
- `NODE_PORT`
- `AGENT_PORT`
- `COORDINATOR_URL`
- `INFER_INTERNAL_KEY` o futura credencial scoped
- `NODE_MODEL`
- `GPU_NAME`, `GPU_VRAM_MB` opcionales si hubiese overrides

## Formato sugerido de config local
### `~/.infer/config.json`
```json
{
  "gatewayUrl": "https://api.infer.example",
  "ollamaUrl": "http://127.0.0.1:11434",
  "nodeName": "my-host",
  "host": "127.0.0.1",
  "agentPort": 8181,
  "ollamaPort": 11434,
  "model": "qwen2.5:0.5b",
  "agentVersion": "0.1.0"
}
```

### `~/.infer/state.json`
```json
{
  "pid": 12345,
  "startedAt": "2026-04-23T12:00:00Z",
  "status": "running",
  "binaryPath": "/Users/alice/.infer/bin/0.1.0/darwin-arm64/node-agent",
  "logPath": "/Users/alice/.infer/logs/node-agent.log",
  "lastHealthCheckAt": "2026-04-23T12:00:10Z"
}
```

## Proceso hijo
### Spawn
La CLI debe lanzar el `node-agent` como child process con entorno controlado.

Recomendaciones:
- `stdio` redirigido a log
- crear directorio de logs si no existe
- en modo detached, desacoplar el proceso de la terminal
- en modo foreground, mostrar logs o stream resumido

### Stop
La CLI debe:
- leer `state.json`
- validar si el pid existe
- intentar terminar el proceso de forma ordenada
- marcar el estado como detenido

## Compatibilidad por fases
### Fase 1
- macOS arm64/amd64
- Linux arm64/amd64
- ejecución local foreground/detached
- checksum SHA256

### Fase 2
- Windows
- servicios del sistema
- update del binario
- rollback de versión

## Integración con CI/CD
### Pipeline de releases del `node-agent`
Debe generar artefactos por plataforma y publicar:
- binario empaquetado
- `checksums.txt`
- metadata de versión

### Pipeline del paquete npm
Debe publicar:
- paquete CLI
- metadata que apunte a la versión de artefactos Go compatible

## Seguridad
### Riesgo actual identificado
El `node-agent` hoy se registra usando `INFER_INTERNAL_KEY`. Eso no es una base adecuada para onboarding abierto de hosts.

### Recomendación
Agregar un flujo de credenciales scoped para nodos, por ejemplo:
- token de enrolamiento de corta vida
- intercambio por credencial de nodo
- uso de credencial de nodo para registro periódico

Mientras eso no exista, la CLI podría operar solo en modo interno/controlado.

## Dependencias técnicas propuestas para la CLI
- `commander` o `oclif`
- `chalk`
- `ora`
- `prompts`
- `undici` o `node-fetch`
- `tar` / `adm-zip` según formato
- `zod` para validación de config/estado

## Riesgos técnicos
1. Divergencia de versiones entre npm CLI y binario Go.
2. Problemas de permisos de ejecución en macOS/Linux.
3. Complejidad de procesos detached en diferentes OS.
4. Validación remota limitada si el gateway no ofrece onboarding formal.
5. Error de UX si el modelo existe en Ollama pero no está permitido por catálogo del gateway.

## Mitigaciones
- Alinear por defecto versión npm y versión del artefacto Go.
- Mantener manifiesto de artefactos por release.
- Implementar mensajes de error muy explícitos.
- Introducir `infer doctor` antes de `infer run` como soporte operacional.
- Documentar claramente el estado de autenticación soportado: interno vs público.

## Estructura inicial de proyecto propuesta
```text
infer-cli/
├─ package.json
├─ README.md
├─ src/
│  ├─ cli.ts
│  ├─ commands/
│  │  ├─ run.ts
│  │  ├─ status.ts
│  │  ├─ stop.ts
│  │  ├─ doctor.ts
│  │  └─ logs.ts
│  ├─ lib/
│  │  ├─ platform.ts
│  │  ├─ artifacts.ts
│  │  ├─ config.ts
│  │  ├─ state.ts
│  │  ├─ ollama.ts
│  │  ├─ gateway.ts
│  │  ├─ process.ts
│  │  └─ paths.ts
│  └─ types/
└─ tsconfig.json
```

## Decisiones abiertas
- Nombre final del paquete npm: `infer`, `@infer/cli` o similar.
- Si la CLI vive en este monorepo o en un repo separado.
- Si el manifiesto de artefactos se resuelve desde GitHub Releases o desde un endpoint propio.
- Si la primera versión será solo para uso interno mientras se resuelve autenticación de hosts.

## Criterios de aceptación de la propuesta
1. Queda definida una arquitectura híbrida npm + binario Go.
2. Existe flujo técnico documentado para `infer run`.
3. Se define una estrategia de publicación de artefactos Go por plataforma.
4. Se define dónde vive la configuración local y cómo se maneja el proceso.
5. Se identifican riesgos y bloqueadores sin implementar cambios de runtime aún.
