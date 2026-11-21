//! # Módulo `ventas`
//!
//! Ventas con ticket consecutivo sin huecos, descuentos con motivo, propina libre (100 % del
//! barbero), pagos, anulación auditada, caja del día, «Lo que llevas hoy» del barbero y
//! cierres de caja (RF-VEN, RF-PBA-03 a 05).
//!
//! Al registrar una venta publica `VentaRegistrada` en la misma transacción: la agenda deja la
//! cita «Atendida» y `clientes` suma la visita. Al anularla publica `VentaAnulada`.

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasVentas, ModuloVentas};

/// Migraciones del esquema `ventas`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
