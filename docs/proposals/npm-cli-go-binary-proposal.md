# Proposal — CLI `infer` distribuida por npm con descarga de binario Go

## Resumen
Se propone construir una **CLI `infer` en Node.js**, distribuida por **npm/npx**, cuya responsabilidad sea simplificar el onboarding y operación local de hosts.

La CLI **no reimplementa el runtime**. En su lugar:
- usa npm para distribución y UX,
- descarga el binario **precompilado** de `node-agent` en Go según la plataforma,
- lo configura y lo ejecuta localmente,
- valida prerequisitos como Ollama, modelo y conectividad al gateway.

La arquitectura recomendada es:
- **CLI**: Node.js / npm
- **Agent runtime**: Go binary descargable
- **Gateway**: Go existente

---

## Problema actual
Hoy conectar un host requiere varios pasos manuales:
- comprobar Ollama y otros prerequisitos,
- descargar el modelo en Ollama,
- compilar o localizar binarios,
- configurar múltiples variables de entorno,
- arrancar `node-agent`,
- verificar que el nodo quedó online.

El repo ya tiene runtime en Go y scripts E2E/manuales, pero no tiene una capa de onboarding simple para hosts.

---

## Objetivo
Permitir una experiencia del estilo:

```bash
npx infer run qwen2.5:0.5b --gateway https://api.infer.example --token <token>
```

O bien, para usuarios frecuentes:

```bash
npm i -g infer
infer run qwen2.5:0.5b --gateway https://api.infer.example --token <token>
```

La CLI debe:
1. validar prerequisitos locales,
2. comprobar que Ollama responde,
3. asegurar que el modelo existe o descargarlo,
4. descargar el `node-agent` correcto,
5. escribir configuración local,
6. arrancar el agente,
7. verificar que el nodo quedó conectado.

---

## Decisión técnica
### Elegimos una arquitectura híbrida

#### CLI en npm/npx
Ventajas:
- distribución mucho más simple,
- ejecución inmediata con `npx`,
- mejor UX para onboarding,
- actualizaciones fáciles,
- ecosistema sólido para prompts, logs y output de consola.

#### `node-agent` en Go
Ventajas:
- ya existe en el repo,
- evita reescribir el runtime,
- encaja bien como proceso persistente,
- mejor continuidad operativa con la arquitectura actual.

---

## Alcance
### Incluido
- nueva CLI `infer` en npm,
- comando principal `infer run <model>`,
- comandos auxiliares:
  - `infer status`
  - `infer stop`
  - `infer doctor`
  - `infer logs`
- descarga automática del binario Go por OS/arquitectura,
- persistencia local de config, state y logs,
- gestión básica del proceso `node-agent`.

### No incluido en esta fase
- reescribir el `node-agent` en Node,
- cambiar el stack del gateway,
- instalar Ollama automáticamente,
- montar servicios del sistema (`systemd`, `launchd`, etc.),
- resolver por completo el modelo de auth pública de hosts,
- auto-update avanzado del binario.

---

## UX propuesta
### Flujo principal
```bash
npx infer run qwen2.5:0.5b --gateway https://api.infer.example --token <token>
```

### Pasos internos
1. detectar plataforma (`darwin/linux/windows` + `arm64/amd64`),
2. validar permisos locales,
3. comprobar acceso a Ollama,
4. comprobar si el modelo existe,
5. hacer pull del modelo si falta,
6. validar reachability del gateway,
7. resolver y descargar el binario del `node-agent`,
8. verificar checksum,
9. guardar config local,
10. arrancar `node-agent`,
11. verificar `health`,
12. verificar registro remoto si aplica,
13. informar éxito.

### Salida esperada
```text
✓ Ollama disponible en http://127.0.0.1:11434
✓ Modelo qwen2.5:0.5b disponible
✓ Binario node-agent descargado (darwin-arm64)
✓ Configuración local escrita
✓ Node agent iniciado
✓ Nodo registrado y online
```

---

## Comandos propuestos
### `infer run <model>`
Bootstrap completo del host.

Flags iniciales sugeridos:
- `--gateway <url>`
- `--token <value>`
- `--ollama-url <url>`
- `--node-name <name>`
- `--host <ip-or-hostname>`
- `--agent-port <port>`
- `--ollama-port <port>`
- `--detach`
- `--foreground`
- `--yes`
- `--no-pull`
- `--agent-version <version>`

### `infer status`
Muestra:
- config actual,
- pid,
- modelo,
- binario en uso,
- estado local del proceso,
- estado remoto si puede consultarse.

### `infer stop`
Detiene el proceso gestionado por la CLI.

### `infer doctor`
Diagnóstico local:
- Ollama,
- puertos,
- permisos,
- reachability del gateway,
- modelo,
- binario.

### `infer logs`
Muestra o sigue logs del `node-agent`.

---

## Distribución del binario Go
### Publicación
Se propone publicar artefactos por versión y plataforma, por ejemplo en GitHub Releases.

Ejemplos:
- `node-agent-darwin-arm64.tar.gz`
- `node-agent-darwin-amd64.tar.gz`
- `node-agent-linux-arm64.tar.gz`
- `node-agent-linux-amd64.tar.gz`
- `node-agent-windows-amd64.zip`
- `checksums.txt`

### Resolución
La CLI debe mapear `process.platform` y `process.arch` a un artefacto concreto.

Ejemplos de target:
- `darwin-arm64`
- `darwin-amd64`
- `linux-arm64`
- `linux-amd64`
- `windows-amd64`

### Integridad
Validación mínima recomendada:
- SHA256 comparado contra `checksums.txt`.

---

## Almacenamiento local propuesto
```text
~/.infer/
├─ bin/
│  └─ <version>/<platform>/node-agent
├─ config.json
├─ state.json
└─ logs/
   └─ node-agent.log
```

### `config.json`
Guarda configuración persistente:
- gateway URL,
- ollama URL,
- modelo,
- puertos,
- nombre del nodo,
- versión del agent.

### `state.json`
Guarda estado operativo:
- pid,
- startedAt,
- status,
- binaryPath,
- logPath,
- último health check.

---

## Gestión del proceso
La CLI debe lanzar el `node-agent` como child process.

### Requisitos
- redirigir stdout/stderr a log,
- guardar PID,
- detectar fallo temprano,
- soportar modo foreground y detached,
- detener el proceso desde `infer stop`.

### Fase inicial
No se instala como servicio del sistema. Solo gestión local del proceso.

---

## Integración con Ollama
La CLI debe interactuar con la API local de Ollama para:
- verificar disponibilidad,
- listar modelos,
- comprobar si el modelo pedido existe,
- descargarlo si falta mediante `/api/pull`.

Esto evita obligar al usuario a hacer pasos manuales antes del onboarding.

---

## Integración con el gateway
La CLI debe poder:
- verificar reachability del gateway,
- usar una credencial o token para onboarding,
- opcionalmente verificar que el nodo quedó registrado.

### Bloqueador actual importante
Hoy el `node-agent` usa `INFER_INTERNAL_KEY` para registrarse.
Eso no es adecuado para onboarding abierto de hosts.

### Recomendación
Introducir en una siguiente fase:
- token de enrolamiento de corta vida,
- intercambio por credencial scoped de nodo,
- uso de esa credencial por el agent.

Mientras eso no exista, la CLI puede nacer primero como herramienta de uso interno/controlado.

---

## Riesgos principales
1. **Autenticación de hosts** aún no resuelta para un flujo público.
2. **Catálogo de modelos** del gateway puede rechazar modelos aunque Ollama los tenga.
3. **Divergencia de versiones** entre npm CLI y binario Go.
4. **Permisos y procesos detached** pueden variar entre plataformas.
5. **UX confusa** si el modelo existe localmente pero no está permitido por plataforma.

---

## Mitigaciones
- alinear por defecto la versión del binario Go con la versión del paquete npm,
- publicar manifiesto/checksums por release,
- hacer `infer doctor` parte central del soporte,
- mostrar errores explícitos de policy/modelo no admitido,
- empezar con macOS/Linux y luego extender.

---

## Fases propuestas
### Fase 1 — MVP interno
- CLI npm funcional,
- descarga binario Go,
- `run/status/stop/doctor/logs`,
- macOS y Linux,
- checksum SHA256,
- uso controlado con credenciales internas.

### Fase 2 — Hardening
- soporte Windows,
- servicios del sistema,
- update/rollback de binarios,
- flujo de enrolamiento scoped para hosts.

### Fase 3 — Self-serve host onboarding
- auth pública de hosts bien definida,
- catálogo/política de modelos más clara,
- experiencia de instalación más robusta y productizada.

---

## Estructura inicial sugerida del proyecto CLI
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

---

## Recomendación final
Avanzar con una **CLI `infer` en npm/npx** y mantener el runtime del `node-agent` en Go descargado como binario por plataforma.

Es la opción con mejor equilibrio entre:
- facilidad de distribución,
- velocidad de iteración,
- buena UX,
- y reutilización del runtime existente.

---

## Criterios de aceptación de esta propuesta
1. queda definida una arquitectura híbrida npm + Go binary,
2. queda documentado el flujo `infer run <model>`,
3. queda definida una estrategia de releases de artefactos Go,
4. queda definido el almacenamiento local de config/state/logs,
5. quedan explicitados los bloqueadores de plataforma antes de implementación completa.
