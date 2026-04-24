# Functional Specification — Infer CLI distribuida por npm con descarga de binarios Go

## Estado
Propuesta. No implementado.

## Contexto
Hoy el onboarding operativo de un host requiere varios pasos manuales:
- comprobar PostgreSQL y Ollama
- descargar el modelo en Ollama
- compilar o ejecutar `api-gateway` y/o `node-agent`
- configurar múltiples variables de entorno
- verificar el registro del nodo y la visibilidad del modelo

El repo ya tiene un runtime estable en Go (`api-gateway`, `node-agent`) y un flujo E2E manual/automatizado en shell, pero no tiene una experiencia de onboarding simple para hosts.

La propuesta es introducir una **CLI `infer` distribuida vía npm/npx** que actúe como capa de UX y orquestación, mientras reutiliza el runtime en Go descargando el binario precompilado del `node-agent` por plataforma.

## Objetivo
Permitir que el host de un modelo pueda conectarse con una experiencia simple, idealmente:

```bash
npx infer run qwen2.5:0.5b --gateway https://api.infer.example --token <token>
```

La CLI debe:
- validar prerequisitos locales
- comprobar acceso a Ollama
- asegurar que el modelo está disponible
- descargar el binario correcto del `node-agent`
- escribir configuración local
- iniciar el proceso agente
- verificar que el host quedó conectado

## Objetivos de negocio y producto
- Reducir fricción de onboarding de hosts.
- Evitar instalación manual de binarios Go por parte del host.
- Mantener el runtime crítico en Go y separar UX/distribución en npm.
- Soportar adopción rápida mediante `npx` y luego instalación global opcional.

## Alcance
### Incluido
- Nueva CLI `infer` publicada en npm.
- Distribución por `npm i -g infer` y `npx infer@latest`.
- Descarga automática del `node-agent` compilado para la plataforma actual.
- Comandos de operación local mínimos:
  - `infer run <model>`
  - `infer status`
  - `infer stop`
  - `infer doctor`
  - `infer logs`
- Persistencia local de config/estado/logs.
- Verificaciones de prerequisitos y conectividad.
- Lanzamiento y supervisión básica del proceso `node-agent`.

### No incluido en esta fase
- Reescritura del `node-agent` en Node.
- Reescritura del `api-gateway`.
- Instalación automática de Ollama.
- Gestión completa como servicio del sistema (`systemd`, `launchd`, Windows Service).
- Auto-update del binario Go en background.
- Implementación de nuevos endpoints del gateway en esta propuesta.

## Usuarios objetivo
### Host/operator técnico
Quiere conectar su máquina a una plataforma Infer con el menor número de pasos posible.

### Developer interno
Quiere probar el flujo de host sin compilar manualmente ni recordar múltiples env vars.

## Experiencia de usuario propuesta
### Flujo principal
```bash
npx infer run qwen2.5:0.5b --gateway https://api.infer.example --token <token>
```

Pasos observables:
1. La CLI detecta sistema operativo y arquitectura.
2. Verifica que Ollama responde.
3. Verifica si el modelo existe localmente.
4. Si el modelo no existe, propone descargarlo o lo descarga según flags.
5. Valida conectividad con el gateway.
6. Intercambia el token o usa las credenciales disponibles para registrar el host.
7. Descarga el binario correcto del `node-agent`.
8. Escribe configuración local del host.
9. Arranca el `node-agent` en segundo plano o foreground según modo.
10. Espera a que el agente se registre correctamente.
11. Informa éxito con datos básicos del nodo.

### Salida esperada
```text
✓ Ollama disponible en http://127.0.0.1:11434
✓ Modelo qwen2.5:0.5b disponible
✓ Binario node-agent descargado (darwin-arm64)
✓ Configuración local escrita
✓ Node agent iniciado
✓ Nodo registrado y online
```

## Comandos funcionales
### `infer run <model>`
Responsabilidad:
- bootstrap completo de un host local para servir un modelo.

Flags iniciales propuestos:
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

### `infer status`
Responsabilidad:
- mostrar estado local del agente y resumen de conectividad.

Debe informar:
- si existe configuración local
- si el proceso está vivo
- modelo configurado
- gateway configurado
- última verificación local
- estado remoto si puede consultarse

### `infer stop`
Responsabilidad:
- detener el agente gestionado por la CLI.

### `infer doctor`
Responsabilidad:
- ejecutar diagnóstico local.

Debe verificar al menos:
- Node.js disponible
- permisos de escritura en el directorio local de infer
- Ollama alcanzable
- modelo descargado o descargable
- reachability del gateway
- puertos ocupados/conflictivos
- binario local utilizable

### `infer logs`
Responsabilidad:
- mostrar o seguir logs del `node-agent` gestionado localmente.

## Requisitos funcionales
### RF-1 Distribución
1. La CLI debe poder ejecutarse mediante `npx` sin instalación global previa.
2. La CLI debe soportar instalación global por npm.
3. La CLI debe resolver el binario correcto del `node-agent` según OS/arquitectura.

### RF-2 Preparación local
1. La CLI debe verificar que Ollama responde antes de iniciar el agente.
2. La CLI debe comprobar la presencia del modelo solicitado.
3. Si el modelo no existe y el modo lo permite, debe iniciar la descarga del modelo.
4. Debe detectar y reportar errores de prerequisitos con mensajes accionables.

### RF-3 Gestión del binario Go
1. La CLI debe descargar el `node-agent` desde un origen versionado y controlado.
2. Debe validar integridad básica del artefacto descargado.
3. Debe guardar el binario en un directorio local estable.
4. Debe evitar re-descargas innecesarias si la versión ya está disponible.

### RF-4 Configuración local
1. La CLI debe persistir configuración local suficiente para reiniciar, consultar estado y detener el agente.
2. La CLI debe persistir estado operacional mínimo: versión, pid, modelo, rutas locales, gateway.
3. La CLI debe almacenar logs localmente.

### RF-5 Ejecución del agente
1. La CLI debe iniciar el `node-agent` con la configuración necesaria.
2. Debe permitir modo foreground y detached.
3. Debe detectar fallo temprano de arranque y mostrar la causa probable.
4. Debe poder detener procesos iniciados por la propia CLI.

### RF-6 Verificación post-arranque
1. La CLI debe verificar el health local del agente.
2. Debe verificar que el nodo quedó registrado si dispone de una forma segura de hacerlo.
3. Debe informar al usuario si el modelo quedó publicado o si el registro falló.

## Requisitos no funcionales
- UX clara y amigable para operadores.
- Mensajes de error orientados a resolución.
- Compatibilidad con macOS y Linux en primera fase.
- Soporte futuro para Windows sin comprometer la arquitectura.
- Descargas reproducibles por versión.
- Separación clara entre capa UX (Node) y runtime crítico (Go).

## Dependencias externas
- npm registry para distribuir la CLI.
- Repositorio de releases para artefactos binarios del `node-agent`.
- Ollama local accesible por HTTP.
- Gateway Infer remoto o local.

## Suposiciones abiertas
- El flujo de autenticación de hosts aún debe definirse con precisión.
- Puede ser necesario introducir `enroll tokens` o credenciales scoped para nodos, en lugar de reutilizar la `INFER_INTERNAL_KEY` global.
- La política de modelos del gateway puede seguir siendo cerrada por catálogo, por lo que la CLI debe reflejar bien los fallos por modelo no admitido.

## Criterios de aceptación de la propuesta
1. Existe una CLI conceptual `infer` con comandos y flujo documentados.
2. La distribución propuesta reutiliza binarios Go precompilados en vez de compilar localmente.
3. La experiencia principal queda definida para `npx infer run <model>`.
4. La separación de responsabilidades entre npm CLI y `node-agent` Go queda explícita.
5. Quedan identificados los bloqueadores de plataforma: autenticación del host, catálogo de modelos y gestión de proceso persistente.
