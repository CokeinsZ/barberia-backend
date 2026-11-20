//! # Módulo `barberos`
//!
//! Equipo: datos, datos privados cifrados, horarios, ausencias, bloqueos, servicios ofrecidos y documentos.
//!
//! **Estado**: fase 0 — el esquema `barberos` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 2 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `barberos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
