# ADR 0002 — Migraciones en la carpeta de cada módulo, aplicadas en un orden global

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

Cada módulo debe tener «sus cosas en su carpeta», incluido el SQL de su esquema, pero hay
llaves foráneas entre esquemas que exigen un orden de aplicación global.

## Decisión

- Cada módulo guarda sus migraciones en `modulos/<modulo>/migraciones/` y las publica con
  `sqlx::migrate!`. La base compartida (extensiones y esquema `nucleo`) vive en
  `crates/plataforma/migraciones/`.
- Las versiones son marcas de tiempo únicas en todo el sistema; el orden de las versiones
  respeta las dependencias (identidad → … → trabajos).
- `diamante migrar` reúne todas con `Migrator::with_migrations` de SQLx 0.9 (API pública),
  valida que no haya versiones repetidas y las aplica en una sola tabla de control.
- `docs/diseno/esquema.sql` se conserva como versión consolidada del diseño.
  `backend/scripts/verificar-esquema.sh` comprueba en CI que las migraciones producen
  exactamente ese esquema y esos datos iniciales, y que pasan las 27 pruebas de reglas de negocio.

## Consecuencias

- Al agregar un módulo con esquema, se registra en `crates/app/src/migraciones.rs` (una prueba
  verifica el orden).
- Un cambio de esquema se hace con una migración nueva en el módulo dueño y, si cambia el
  diseño, también en `esquema.sql`; el script detecta si divergen.
