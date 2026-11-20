//! # Módulo `notificaciones`
//!
//! Cola de salida de mensajes, WhatsApp Cloud API, correo SMTP, webhooks y avisos internos.
//!
//! **Estado**: fase 0 — el esquema `notificaciones` de la base de datos está listo en `migraciones/`. El código se construye en la
//! fase 4 con la misma estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

use sqlx::migrate::Migrator;

/// Migraciones del esquema `notificaciones`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: Migrator = sqlx::migrate!("./migraciones");
