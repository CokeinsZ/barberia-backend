//! # Módulo `archivos`
//!
//! Almacenamiento de fotos y documentos en el volumen de archivos y URL firmadas.
//!
//! **Estado**: fase 0 — el esquema `archivos` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 1 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `archivos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
