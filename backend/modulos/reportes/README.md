# Módulo `reportes`

Dashboard, reportes, liquidaciones, PDF y Excel. Solo lectura: no tiene esquema propio y lee las vistas publicadas (`v_`) de los demás módulos.

| | |
|---|---|
| Fase del plan | 6 |
| Esquema de base de datos | — (lee vistas publicadas) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 6. |

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
