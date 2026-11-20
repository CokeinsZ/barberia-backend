//! Adaptador HTTP del módulo: rutas, manejadores, cuerpos de las solicitudes, límites de
//! intentos y el validador de sesiones que usa el resto de la API.

mod limites;
mod manejadores;
mod rutas;
mod sesiones;
mod solicitudes;

pub(crate) use limites::Limites;
pub(crate) use rutas::{EstadoHttp, rutas};
pub(crate) use sesiones::ValidadorIdentidad;
