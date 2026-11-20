# Módulo `verificacion`

Códigos de verificación por WhatsApp y comprobantes de un solo uso.

| | |
|---|---|
| Fase del plan | 3 |
| Esquema de base de datos | `verificacion` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 3. |

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
