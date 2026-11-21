//! # Módulo `clientes`
//!
//! Registro único por celular (RF-CLI-01, RN-08), creado al reservar en línea (con la prueba de la
//! autorización, RF-RES-11) o desde el panel; búsqueda, listado con segmentos (RN-20), ficha y
//! edición. Las métricas son una proyección: las inasistencias llegan por el evento
//! `InasistenciaMarcada` (y las visitas y el gasto, por las ventas en la fase 5).

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasClientes, ModuloClientes};

/// Migraciones del esquema `clientes`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
