//! # Módulo `archivos`
//!
//! Almacenamiento de fotos y documentos en el volumen de archivos (`DIAMANTE_ARCHIVOS_DIR`) y
//! URL firmadas (RNF-SEG-09).
//!
//! - Los módulos dueños (p. ej. `barberos`) guardan con [`api::Archivos::guardar`] dentro de su
//!   transacción: aquí se valida el tipo **por su contenido** (no por la extensión) y el tamaño.
//! - **Públicos** (fotos de barberos): `/media/publico/…`, que sirve la API con caché de un
//!   año (las claves son aleatorias y nunca se reutilizan).
//! - **Privados** (documentos, soportes): solo con una URL firmada (HMAC-SHA256) de corta
//!   duración, `GET /api/v1/archivos/{id}?exp=…&firma=…`.

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasArchivos, ModuloArchivos};

/// Migraciones del esquema `archivos`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
