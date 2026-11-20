//! # Módulo `identidad`
//!
//! Cuentas del personal, sesiones, invitaciones, recuperación de contraseña y administradores.
//!
//! **Estado**: fase 0 — el esquema `identidad` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 1 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `identidad`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
