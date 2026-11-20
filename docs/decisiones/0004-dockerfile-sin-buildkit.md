# ADR 0004 — Dockerfile compatible con el constructor clásico

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

Los montajes de caché (`RUN --mount=type=cache`) exigen BuildKit (`docker buildx`), que no
viene en todas las instalaciones de Docker (por ejemplo, la máquina de desarrollo actual).

## Decisión

`infra/docker/backend.Dockerfile` conserva la caché de dependencias sin BuildKit: una etapa
extrae solo los manifiestos de Cargo, otra compila un esqueleto con las dependencias y luego se
compila el código real. La imagen final es `debian:trixie-slim` con un usuario sin privilegios.

## Consecuencias

- `docker compose up --build` funciona en cualquier instalación.
- Un cambio de código recompila solo los crates propios (≈ 1 minuto en la máquina de desarrollo).
