//! # Módulo `trabajos`
//!
//! Cola de tareas en segundo plano y planificador del worker.
//!
//! **Estado**: fase 0 — el esquema `trabajos` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 4 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `trabajos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
