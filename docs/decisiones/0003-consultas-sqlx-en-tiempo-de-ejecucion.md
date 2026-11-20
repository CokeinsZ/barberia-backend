# ADR 0003 — Consultas de SQLx verificadas en tiempo de ejecución

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

Las macros `sqlx::query!` validan el SQL al compilar, pero necesitan una base de datos
disponible al compilar o los metadatos de `cargo sqlx prepare` versionados, lo que complica
la imagen de Docker y el CI.

## Decisión

Se usan `sqlx::query`, `query_as` y `query_scalar` con SQL literal (`&'static str`, que SQLx 0.9
exige mediante `SqlSafeStr`) y estructuras `FromRow`. Cada repositorio se cubre con pruebas de
integración contra PostgreSQL real (`tests/` del módulo, base efímera de `plataforma`).

## Consecuencias

- La compilación no depende de la base de datos; la imagen se construye en cualquier máquina.
- Un error de SQL se detecta en las pruebas de integración, no al compilar. Se puede
  reevaluar más adelante con `cargo sqlx prepare` si el número de consultas crece.
