//! # Módulo `notificaciones`
//!
//! Cola de salida de mensajes (patrón *outbox*): los demás módulos encolan con
//! [`api::BuzonSalida`] **dentro de su transacción** y el worker los envía después. Así un
//! mensaje nunca se envía por una operación que se revirtió, ni se pierde si el proveedor falla.
//!
//! - Correo por SMTP (invitaciones y recuperación de contraseña).
//! - WhatsApp: plantillas, reintentos y, mientras no haya credenciales de Meta, un adaptador
//!   simulado fuera de producción.
//! - Webhooks firmados de WhatsApp: estados de entrega y respuestas de los botones, que se
//!   publican como `RespuestaWhatsAppRecibida`.
//! - Avisos internos para la administradora ([`api::Avisos`]).

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasNotificaciones, ModuloNotificaciones};

/// Migraciones del esquema `notificaciones`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
