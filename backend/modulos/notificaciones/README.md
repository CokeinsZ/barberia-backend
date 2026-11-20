# Módulo `notificaciones`

Cola de salida de mensajes, WhatsApp Cloud API, correo SMTP, webhooks y avisos internos.

| | |
|---|---|
| Fase del plan | 4 |
| Esquema de base de datos | `notificaciones` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 4. |

## Eventos que publica (crate `contratos`)

- `RespuestaWhatsAppRecibida`

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
