# Módulo `ventas`

Ventas, pagos, descuentos, caja del día y cierres de caja.

| | |
|---|---|
| Fase del plan | 5 |
| Esquema de base de datos | `ventas` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 5. |

## Eventos que publica (crate `contratos`)

- `VentaRegistrada`
- `VentaAnulada`

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
