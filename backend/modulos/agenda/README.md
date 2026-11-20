# Módulo `agenda`

Disponibilidad, asignación de barbero, citas, enlaces de gestión y recordatorios.

| | |
|---|---|
| Fase del plan | 3 |
| Esquema de base de datos | `agenda` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 3. |

## Eventos que publica (crate `contratos`)

- `CitaCreada`
- `CitaReprogramada`
- `CitaCancelada`
- `InasistenciaMarcada`

## Eventos que consume

- VentaRegistrada (ventas): la cita queda «Atendida»
- HorarioCambiado, BloqueoCreado, AusenciaRegistrada (barberos): aviso si hay citas afectadas
- RespuestaWhatsAppRecibida (notificaciones): confirma o cancela la cita

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
