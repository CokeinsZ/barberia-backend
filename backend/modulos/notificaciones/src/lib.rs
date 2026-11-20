//! # Módulo `notificaciones`
//!
//! Cola de salida de mensajes (patrón *outbox*): los demás módulos encolan con
//! [`api::BuzonSalida`] **dentro de su transacción** y el worker los envía después. Así un
//! correo nunca se envía por una operación que se revirtió, ni se pierde si el proveedor falla.
//!
//! Fase 1: correo por SMTP (invitaciones y recuperación de contraseña). Fase 4: WhatsApp,
//! webhooks y avisos internos.

mod aplicacion;
mod dominio;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::ModuloNotificaciones;

/// Migraciones del esquema `notificaciones`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
