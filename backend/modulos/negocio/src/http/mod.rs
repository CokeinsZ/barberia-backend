//! Traducción entre HTTP y los casos de uso. Aquí no hay reglas de negocio.

mod manejadores;
mod respuestas;
mod rutas;
mod solicitudes;

pub(crate) use rutas::rutas;
