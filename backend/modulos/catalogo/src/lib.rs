//! # Módulo `catalogo`
//!
//! Servicios simples, combos y categorías.
//!
//! **Estado**: fase 0 — el esquema `catalogo` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 2 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `catalogo`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
