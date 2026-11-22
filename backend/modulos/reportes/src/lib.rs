//! # Módulo `reportes`
//!
//! Dashboard, resumen de ventas, liquidación por barbero, reporte de clientes y sus PDF
//! (RF-REP, RF-DAS, RF-PBA-06). Solo lectura: no tiene esquema propio y lee las **vistas
//! publicadas** (`v_`) de los demás módulos; la única escritura es la auditoría de cada PDF
//! (RF-SIS-07). Los PDF los genera Gotenberg (HTML → PDF con Chromium).

mod aplicacion;
mod dominio;
mod http;
mod infraestructura;
mod modulo;

pub mod api;

pub use modulo::{DependenciasReportes, ModuloReportes};
