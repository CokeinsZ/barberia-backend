# Módulo `auditoria`

Registro de solo inserción de las acciones sensibles.

| | |
|---|---|
| Fase del plan | 1 |
| Esquema de base de datos | `auditoria` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 1. |

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
