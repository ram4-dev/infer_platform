# RFC-001 — Simplificación de inferencia: **1 nodo = 1 GPU = 1 modelo**

**Estado:** Propuesta (pre-implementación)  
**Fecha:** 2026-04-22  
**Autores:** Equipo Infer Platform

---

## 1) Contexto

Hoy la plataforma conserva una arquitectura de *shard planning* y pipeline multi-nodo para un mismo request/modelo. En la práctica actual (MVP con Ollama), ese pipeline no aporta paralelismo tensorial real y agrega complejidad operativa.

Queremos pasar temporalmente a un modo más simple, robusto y predecible:

> **Cada nodo atiende inferencia completa de un único modelo en una única GPU.**

El gateway solo debe decidir **a qué nodo enrutar** una request, siempre dentro del conjunto de nodos que sirven ese mismo modelo.

---

## 2) Objetivo

### Objetivo funcional

- Eliminar, por ahora, la ejecución multi-nodo de un mismo modelo.
- Enrutar requests por modelo a nodos compatibles.
- Mantener balanceo + failover, pero **solo entre nodos del mismo modelo**.

### Objetivo no funcional

- Reducir errores y complejidad del path crítico (`/v1/chat/completions`).
- Mejorar trazabilidad operativa.
- Alinear arquitectura con capacidades reales del backend actual (Ollama).

---

## 3) Decisión de arquitectura

### Regla principal

- **1 nodo = 1 modelo activo** (en esta fase).
- **1 request = 1 nodo**.
- No hay `shard plan` para dividir capas entre nodos.

### Implicancias

- Se elimina del flujo runtime de chat:
  - selección de `ShardPlan` multi-nodo,
  - forwarding `/infer/shard` entre agentes para combinar ejecución,
  - lógica de controller/worker por request.

- Se reemplaza por:
  - selección de candidatos por `model` + `status=online`,
  - ordenamiento por salud/latencia,
  - round-robin por modelo,
  - failover al siguiente nodo del mismo modelo si falla.

---

## 4) Flujo propuesto de request

1. Cliente llama `POST /v1/chat/completions` con `model`.
2. Gateway valida API key + rate limit.
3. Gateway busca nodos `online` que anuncian ese `model`.
4. Gateway arma lista de candidatos del mismo modelo:
   - orden base: menor p50 latencia,
   - rotación round-robin por modelo,
   - fallback secuencial si falla nodo elegido.
5. Gateway proxea request al Ollama del nodo seleccionado.
6. Devuelve respuesta OpenAI-compatible (stream/non-stream).

**Importante:** no se permite fallback a otro modelo.

---

## 5) Cambios sustanciales por componente

## 5.1 `cmd/api-gateway` + `internal/gateway`

### Chat routing

- Sustituir path actual de `ShardCoordinator` por `ModelRouter` (o equivalente).
- `ModelRouter` debe:
  - filtrar por modelo exacto normalizado,
  - usar solo nodos `online`,
  - balancear entre pares del mismo modelo,
  - aplicar failover dentro del mismo modelo.

### Registro/consulta de nodos

- Usar fuente única en modo DB (PostgreSQL) para evitar desincronización con estado in-memory.
- Mantener in-memory solo como fallback dev sin DB.

### Salud y métricas

- Reutilizar health monitor actual.
- Incluir métricas por modelo:
  - `requests_by_model`,
  - `failover_count_by_model`,
  - `no_capacity_by_model`.

---

## 5.2 `cmd/node-agent` + `internal/nodeagent`

### Registro

- El nodo debe registrar **exactamente un modelo activo**.
- Propuesta de contrato:
  - `model` (string, obligatorio),
  - `license` (string, obligatorio según política vigente).

> Alternativa compatible: mantener `models: []` en API pero validar `len == 1` en gateway.

### Variables de entorno sugeridas

- `NODE_MODEL=llama3.1:8b`
- `NODE_MODEL_LICENSE=llama-3.1`

---

## 5.3 Capacidades de sharding futuro

- Quedan fuera del path crítico de inferencia durante esta fase.
- No forman parte del runtime actual del repo; cualquier reintroducción futura deberá hacerse sobre el stack Go y con un backend que soporte paralelismo real.

---

## 5.4 `web/`

- Ajustar labels/UI:
  - dejar de comunicar “sharding de un mismo request en múltiples nodos” como comportamiento actual,
  - mostrar disponibilidad por modelo y cantidad de nodos por modelo.

---

## 6) Contrato de datos

## 6.1 Registro de nodo

### Antes
- `models: []` opcional y no siempre informado por clientes.

### Después
- `model` obligatorio (o `models` con cardinalidad 1 obligatoria).
- Validación estricta de licencia continúa vigente.

## 6.2 Persistencia

Opciones:

1. **Rápida (mínima migración):** mantener `node_models`, garantizar 1 fila activa por nodo desde lógica de aplicación.
2. **Estructural:** agregar `nodes.primary_model` y `nodes.primary_license`; dejar `node_models` para histórico/multi-modelo futuro.

Recomendación: opción 1 para el primer corte.

---

## 7) Compatibilidad y migración

### Etapa A — Preparación
- Introducir feature flag (ej: `ROUTING_MODE=single_node_model`).
- Actualizar node-agent/CLI para enviar modelo obligatorio.

### Etapa B — Corte funcional
- Activar nuevo router en gateway.
- Desactivar path multi-nodo para chat.

### Etapa C — Limpieza
- Marcar `ShardCoordinator` multi-nodo como deprecated en docs.
- Mantener código detrás de flag o remover en siguiente release.

---

## 8) Criterios de aceptación

1. Si llegan 100 requests del modelo `X` y hay 3 nodos online con `X`, todas se enrutan a esos 3 nodos (balanceadas).
2. Si cae un nodo de `X`, el tráfico continúa en los nodos restantes de `X` sin error sistémico.
3. Si no hay nodos de `X`, respuesta clara (`503/502`) indicando modelo no disponible.
4. No se intenta ejecutar un mismo request en múltiples nodos.
5. Streaming y non-streaming usan la misma selección de nodo (misma política de routing).

---

## 9) Riesgos y mitigaciones

- **Riesgo:** menor capacidad para modelos gigantes que requerían visión de sharding.
  - **Mitigación:** declarar explícitamente catálogo soportado por modelo/nodo.

- **Riesgo:** nodos registrados sin modelo válido.
  - **Mitigación:** validación obligatoria en `register` + rechazo 422.

- **Riesgo:** inconsistencia DB vs memoria.
  - **Mitigación:** en modo DB, leer/escribir nodos solo desde DB en runtime.

---

## 10) Out of scope (esta fase)

- Tensor parallelism real multi-nodo.
- Scheduling por costo energético/región.
- Multi-modelo por nodo con fairness avanzada.
- Pipeline inter-node `/infer/shard` para producción.

---

## 11) Resumen ejecutivo

Esta RFC propone priorizar **confiabilidad y simplicidad operativa** sobre complejidad de paralelismo no materializada en el backend actual. El sistema pasa a un modelo de enrutamiento directo por modelo:

- **Simple:** 1 request, 1 nodo.
- **Predecible:** failover solo entre nodos del mismo modelo.
- **Escalable:** se escala agregando más nodos que sirvan el mismo modelo.

Es el paso recomendado antes de retomar sharding real con un backend que sí soporte layer/tensor split de forma nativa.
