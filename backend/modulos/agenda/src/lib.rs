//! # Módulo `agenda`
//!
//! El corazón del producto: reservar sin cruces.
//!
//! - **Motor de disponibilidad** (`dominio::disponibilidad`, función pura con pruebas
//!   exhaustivas): franjas de cada barbero, días especiales (festivo con horario de domingo,
//!   cerrado, cierre temprano), ausencias, retiro, bloqueos, citas, duración del servicio,
//!   intervalo, anticipación mínima y ventana de reserva (RF-SIS-01). Con «Cualquier barbero»
//!   asigna el primero libre según el orden del equipo (RN-06).
//! - **Reserva en línea** atómica con el comprobante de `verificacion`; si la hora se toma entre
//!   medio, la restricción de exclusión de PostgreSQL lo impide (RNF-INT-01) y la respuesta trae
//!   horas alternativas.
//! - **Gestión por enlace** (ver, cancelar, reprogramar, recordatorios), **agenda del personal**
//!   y la **landing** (lee las fachadas de `negocio`, `catalogo` y `barberos`).
//!
//! Los mensajes de WhatsApp quedan en la cola de `notificaciones`; el envío real llega en la fase 4.

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasAgenda, ModuloAgenda};

/// Migraciones del esquema `agenda`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
