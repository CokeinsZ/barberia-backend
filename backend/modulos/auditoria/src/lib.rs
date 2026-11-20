//! # Módulo `auditoria`
//!
//! Registro de solo inserción de las acciones sensibles (RF-SIS-07, RNF-SEG). Los demás
//! módulos escriben con [`api::Auditor`] **dentro de su propia transacción**, así la acción y
//! su registro se confirman o se revierten juntos. El administrador lo consulta con
//! `GET /api/v1/auditoria`.
//!
//! No tiene capa de dominio propia: no hay reglas de negocio, solo registro y consulta.

mod aplicacion;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::ModuloAuditoria;

/// Migraciones del esquema `auditoria`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
