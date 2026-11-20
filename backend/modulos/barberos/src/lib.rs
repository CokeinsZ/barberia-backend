//! # Módulo `barberos`
//!
//! Equipo: datos, datos privados cifrados, horarios, ausencias, bloqueos, servicios ofrecidos y documentos.
//!
//! **Estado**: el esquema `barberos` está listo en `migraciones/`. En la fase 1 solo existe el
//! adaptador que le dice al módulo `identidad` quién es el barbero de una cuenta y si puede
//! entrar al panel ([`identidad::api::PerfilesBarberos`]). El resto se construye en la fase 2 con
//! la estructura del módulo `negocio` (ver `docs/plan-de-desarrollo.md`, sección 4.1).

mod infraestructura;
mod modulo;

pub use modulo::ModuloBarberos;

/// Migraciones del esquema `barberos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
