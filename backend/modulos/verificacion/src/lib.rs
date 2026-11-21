//! # Módulo `verificacion`
//!
//! Verifica el celular de quien reserva en línea (RF-RES-09, RNF-SEG-06): un código de 4 dígitos
//! por WhatsApp que vence en 10 minutos, admite 5 intentos y solo se puede reenviar tras 60
//! segundos. En la base solo queda su HMAC con la clave del servidor. Al acertar entrega un
//! **comprobante** de un solo uso (30 minutos) que `agenda` consume al crear la cita.
//!
//! Los datos del cliente conocido se muestran **después** de verificar (RF-RES-08); los aporta el
//! puerto [`api::ClientesConocidos`].

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasVerificacion, ModuloVerificacion};

/// Migraciones del esquema `verificacion`.
pub static MIGRACIONES: sqlx::migrate::Migrator = sqlx::migrate!("./migraciones");
