//! Reglas de los servicios, sin dependencias de infraestructura.

mod servicio;

pub(crate) use servicio::{DatosServicio, PropuestaServicio, TipoServicio, validar_componentes};
