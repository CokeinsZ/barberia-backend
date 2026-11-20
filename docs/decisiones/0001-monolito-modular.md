# ADR 0001 — Monolito modular con un crate por módulo

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

El sistema tiene 13 contextos de negocio (agenda, ventas, barberos…) y un equipo pequeño. Se
quiere código «en extremo modular», con cada módulo casi independiente, pero sin el costo
operativo de varios servicios.

## Decisión

- Un solo despliegue (monolito) dividido en **un crate de Cargo por módulo** (`backend/modulos/*`),
  más tres crates transversales (`nucleo`, `contratos`, `plataforma`) y la raíz de composición
  (`app`).
- Cada crate publica solo `api` (fachada), su tipo `Modulo…` y `MIGRACIONES`; el resto es
  `pub(crate)`.
- Capas por módulo: `dominio`, `aplicacion`, `infraestructura`, `http` (arquitectura hexagonal).
- Un esquema de PostgreSQL por módulo; solo el dueño escribe en él.
- Los ciclos entre módulos se rompen con eventos (`contratos`) o con puertos implementados en `app`.

## Consecuencias

- El compilador hace cumplir las fronteras: un módulo no puede usar el interior de otro y
  Cargo rechaza los ciclos.
- Compilación incremental por crate.
- Más archivos y algo de ceremonia por módulo; se mitiga con la plantilla (`negocio`) y el
  script `backend/scripts/estructurar-modulo.sh`.
