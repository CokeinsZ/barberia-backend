# Módulo `barberos`

Equipo: datos, datos privados cifrados, horarios, ausencias, bloqueos, servicios ofrecidos y documentos.

| | |
|---|---|
| Fase del plan | 2 |
| Esquema de base de datos | `barberos` (ver `migraciones/`) |
| Estado | Esqueleto: solo migraciones. Se implementa en la fase 2. |

## Eventos que publica (crate `contratos`)

- `HorarioCambiado`
- `BloqueoCreado`
- `AusenciaRegistrada`
- `AccesoRevocado`

## Eventos que consume

- Ninguno.

## Estructura

Al implementarlo se sigue la plantilla del módulo de referencia [`negocio`](../negocio/README.md):
`dominio/`, `aplicacion/`, `infraestructura/`, `http/`, `api.rs` (fachada pública), `modulo.rs`
(construcción) y `tests/`. Solo `api`, `modulo` y `MIGRACIONES` son públicos.
