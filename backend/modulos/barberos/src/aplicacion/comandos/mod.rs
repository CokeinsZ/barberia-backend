mod equipo;
mod expediente;
mod horario_servicios;
mod novedades;
mod registro;

pub(crate) use equipo::{CambiarVisibilidad, OrdenarEquipo, RetirarBarbero, SolicitudVisibilidad};
pub(crate) use expediente::{CambiarFoto, GestionDatosPrivados, GestionDocumentos};
pub(crate) use horario_servicios::{GestionHorario, GestionServicios};
pub(crate) use novedades::{GestionAusencias, GestionBloqueos, SolicitudBloqueo};
pub(crate) use registro::{EditarBarbero, RegistrarBarbero, SolicitudRegistro};
