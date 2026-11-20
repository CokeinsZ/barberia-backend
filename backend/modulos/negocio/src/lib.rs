//! # Módulo `negocio`
//!
//! Datos de la barbería (nombre, NIT, contacto, comisión por defecto) y días especiales
//! (festivos, cierres y cierres tempranos).
//!
//! Es el **módulo de referencia** de la arquitectura: todos los módulos siguen esta misma
//! estructura de capas.
//!
//! - `dominio`: entidades con sus reglas. No conoce SQL, HTTP ni otros módulos.
//! - `aplicacion`: un archivo por caso de uso, puertos (traits) y políticas de autorización.
//! - `infraestructura`: implementación de los puertos con PostgreSQL.
//! - `http`: rutas y traducción entre HTTP y los casos de uso.
//!
//! Solo son públicos [`api`] (lo que otros módulos pueden usar), [`ModuloNegocio`]
//! (construcción, para la raíz de composición) y [`MIGRACIONES`].

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::ModuloNegocio;

/// Migraciones del esquema `negocio`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
