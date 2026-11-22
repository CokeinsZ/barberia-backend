//! Fachada pública del módulo `reportes`. Ningún módulo lo usa: es la última capa de lectura.

pub use crate::aplicacion::dto::{DashboardDto, ReporteBarberoDto, ReporteClientesDto, ResumenDto};
pub use crate::aplicacion::errores::ErrorReportes;
pub use crate::dominio::periodo::Periodo;
