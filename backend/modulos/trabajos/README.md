# Módulo `trabajos`

Cola de tareas en segundo plano y planificador del worker.

| | |
|---|---|
| Fase del plan | 4 ✅ |
| Esquema de base de datos | `trabajos` (ver `migraciones/`) |
| Fachada | `api::Tarea` (lo que se ejecuta), `api::Programacion` (cada cuánto) y `ModuloTrabajos::pasada()` |
| Quién registra las tareas | `app/src/tareas.rs`, con las fachadas de los módulos: este módulo no conoce a ninguno |

## Cómo funciona

1. **Planificar**: en cada pasada del worker (cada 5 s), cada tarea periódica se programa para
   la **franja vigente** con una clave única (`agenda.recordatorios@2026-11-21T15:05:00Z`).
   Dos pasadas seguidas, o dos workers, no la duplican (E2E-NOT-01). Si el worker estuvo
   detenido, solo se recupera la franja vigente.
2. **Tomar**: los trabajos listos se toman con `FOR UPDATE SKIP LOCKED` y quedan `en_proceso`
   con quién los tomó (`tomado_por`).
3. **Ejecutar**: si la tarea falla, se reintenta a 1, 5, 15 y 60 minutos hasta `max_intentos`;
   después queda `fallido`. Un trabajo «en proceso» por más de 15 minutos se da por abandonado
   y vuelve a la cola.

| Tarea | Cada cuánto | Qué hace |
|---|---|---|
| `agenda.recordatorios` | 5 minutos | Recordatorios de 24 h y 2 h (RF-NOT-02) |
| `sistema.mantenimiento` | 3:00 a. m. de Bogotá (RNF-DIS-02) | Borra sesiones y enlaces de acceso vencidos, códigos viejos, enlaces de citas pasadas, webhooks procesados y trabajos completados hace más de 30 días |

**Vigilancia** (RNF-OBS-02): `ModuloTrabajos::estado()` da los trabajos vencidos sin tomar (y el
atraso del más antiguo), los atascados en proceso, los fallidos de las últimas 24 horas y los
minutos desde que terminó el último. Con el worker en marcha, los recordatorios terminan cada 5
minutos: más de 15 sin ningún trabajo terminado (o algo vencido hace 10) es la alerta
`worker_detenido` de `GET /api/salud/operacion`.

## Eventos

- Publica: ninguno. Consume: ninguno.

## Estructura

```text
trabajos/src/
├── api.rs               # Tarea, ContextoTarea, Programacion, Periodicidad, ResumenTrabajos
├── modulo.rs            # ModuloTrabajos::pasada() y limpiar_cola()
├── dominio/             # franja vigente, clave de idempotencia y espera entre reintentos
├── aplicacion/          # Ejecutor: planificar, recuperar abandonados y ejecutar
└── infraestructura/     # RepositorioTrabajosPg
```
