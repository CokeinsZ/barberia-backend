//! # Módulo `identidad`
//!
//! Cuentas del personal (administradores y barberos), sesiones en el servidor, invitaciones,
//! recuperación de contraseña y gestión de administradores. Los clientes no tienen cuenta.
//!
//! - **Sesiones**: token aleatorio de 256 bits en la cookie `__Host-sesion`; en la base solo se
//!   guarda su SHA-256. Vencen tras 12 horas sin uso, o 30 días con «Recordarme».
//! - **Bloqueo**: 5 contraseñas fallidas bloquean la cuenta 15 minutos.
//! - **Enlaces** de activación (7 días) y recuperación (1 hora): de un solo uso; solo el último
//!   enlace emitido sirve.
//!
//! Los datos del barbero (nombre e identificador) son del módulo `barberos`, que implementa el
//! puerto [`api::PerfilesBarberos`].

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasIdentidad, ModuloIdentidad};

/// Migraciones del esquema `identidad`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
