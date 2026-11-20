# Módulo `identidad`

Cuentas del personal, sesiones, invitaciones, recuperación de contraseña y administradores.

| | |
|---|---|
| Fase del plan | 1 |
| Esquema de base de datos | `identidad` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 1. |

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- AccesoRevocado (barberos): desactiva la cuenta y cierra sesiones

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
