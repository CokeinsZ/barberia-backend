//! # Módulo `catalogo`
//!
//! Servicios simples y combos (RF-SER-01 a 05 y 08). Un combo agrupa dos o más servicios
//! simples y tiene precio y duración propios; se agenda y se vende como un solo servicio
//! (RN-28, DA-02). Un cambio de precio solo afecta lo nuevo: citas y ventas guardan el precio
//! con el que se crearon (RF-SER-03).
//!
//! Cuántos barberos ofrecen cada servicio lo responde el módulo `barberos` a través del puerto
//! [`api::OferentesServicio`] (inversión de dependencias: `barberos` depende de `catalogo`).

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasCatalogo, ModuloCatalogo};

/// Migraciones del esquema `catalogo`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
