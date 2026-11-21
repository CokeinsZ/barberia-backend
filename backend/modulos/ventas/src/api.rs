//! Fachada pública del módulo `ventas`. Los demás módulos no lo llaman: reaccionan a sus
//! eventos (`VentaRegistrada`, `VentaAnulada`, crate `contratos`).

pub use crate::aplicacion::dto::{CajaDto, CierreDto, ResumenBarberoDto, VentaDto};
pub use crate::aplicacion::errores::ErrorVentas;
