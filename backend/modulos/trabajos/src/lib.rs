//! # Módulo `trabajos`
//!
//! Cola de tareas en segundo plano y planificador del worker. Las tareas periódicas
//! ([`api::Programacion`]) se programan una vez por franja con una clave única, así varias
//! pasadas o varios workers no las duplican; cada trabajo se toma con `FOR UPDATE SKIP LOCKED`,
//! se reintenta con espera creciente y, si el proceso se cae, vuelve a la cola.
//!
//! Las tareas ([`api::Tarea`]) las implementa la raíz de composición (`app`) con las fachadas
//! de los módulos: este módulo no conoce a ninguno.

mod aplicacion;
mod dominio;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{ModuloTrabajos, limpiar_cola};

/// Migraciones del esquema `trabajos`. Las aplica el comando `migrar` junto con las de los demás módulos.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
