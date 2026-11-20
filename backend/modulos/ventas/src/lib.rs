//! # Módulo `ventas`
//!
//! Ventas, pagos, descuentos, caja del día y cierres de caja.
//!
//! **Estado**: fase 0 — el esquema `ventas` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 5 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `ventas`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
