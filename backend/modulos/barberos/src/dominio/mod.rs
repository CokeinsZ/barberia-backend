//! Reglas del equipo, sin dependencias de infraestructura.

mod datos;
mod expediente;
mod horario;
mod novedades;

pub(crate) use datos::{DatosBarbero, PropuestaBarbero};
pub(crate) use expediente::{DatosPrivados, PropuestaPrivados, TipoAdjunto, periodo_planilla};
pub(crate) use horario::{Franja, Horario};
pub(crate) use novedades::{EstadoBarbero, NuevaAusencia, NuevoBloqueo, Retiro};

/// Texto opcional: vacío o solo espacios cuenta como ausente; los espacios repetidos se unen.
pub(crate) fn texto_opcional(texto: Option<String>) -> Option<String> {
    texto
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
}

/// Texto obligatorio normalizado (espacios repetidos unidos).
pub(crate) fn texto(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}
