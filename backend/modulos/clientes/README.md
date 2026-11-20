# Módulo `clientes`

Clientes, autorizaciones de datos, habeas data, segmentos y proyección de métricas.

| | |
|---|---|
| Fase del plan | 3 |
| Esquema de base de datos | `clientes` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 3. |

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- VentaRegistrada, VentaAnulada (ventas): actualizan la proyección `clientes.metricas`
- InasistenciaMarcada (agenda): suma inasistencias

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
