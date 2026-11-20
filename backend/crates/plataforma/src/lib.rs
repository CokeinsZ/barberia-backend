//! # Plataforma
//!
//! Infraestructura técnica que comparten los módulos: configuración, conexión y
//! transacciones de PostgreSQL, migraciones combinadas, traducción de errores a HTTP,
//! sesión y telemetría. No contiene reglas de negocio.
//!
//! Los módulos la usan solo desde sus capas `infraestructura` y `http`; su `dominio` no la conoce.

pub mod base_datos;
pub mod configuracion;
pub mod http;
pub mod telemetria;
