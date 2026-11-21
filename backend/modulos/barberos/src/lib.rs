//! # Módulo `barberos`
//!
//! El equipo (RF-BAR-01 a 19 y RF-PBA-07 a 10): datos esenciales y privados (cifrados), cuenta
//! del panel, horario, servicios que ofrece, ausencias, bloqueos, retiro, visibilidad, orden,
//! foto y documentos.
//!
//! - El **estado** (activo, vacaciones, incapacidad o licencia, suspendido, inactivo) no se
//!   guarda: se calcula para cada fecha con `barberos.estados_en` (RN-24). Por eso un barbero
//!   vuelve solo a «Activo» cuando termina su ausencia (E2E-BAR-03).
//! - Los **cruces** de ausencias, bloqueos y franjas los impide la base de datos con
//!   restricciones de exclusión; el módulo los traduce a errores 409.
//! - Publica `HorarioCambiado`, `BloqueoCreado`, `AusenciaRegistrada` y `AccesoRevocado` (crate
//!   `contratos`) para que `agenda` (fase 3) avise de las citas afectadas.

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasBarberos, ModuloBarberos};

/// Migraciones del esquema `barberos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
