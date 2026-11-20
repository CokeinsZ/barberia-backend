use chrono::NaiveDate;
use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

use crate::dominio::errores::ErrorDominio;

/// Errores de los casos de uso del módulo `negocio`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorNegocio {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Ya hay un día especial registrado el {}.", .0.format("%d/%m/%Y"))]
    FechaOcupada(NaiveDate),
    #[error("El día especial no existe.")]
    DiaEspecialNoEncontrado,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl From<ErrorDominio> for ErrorNegocio {
    fn from(error: ErrorDominio) -> Self {
        match error {
            ErrorDominio::DatosInvalidos(errores) => ErrorNegocio::DatosInvalidos(errores),
        }
    }
}

impl ErrorDeAplicacion for ErrorNegocio {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorNegocio::DatosInvalidos(_) => Categoria::Validacion,
            ErrorNegocio::FechaOcupada(_) => Categoria::Conflicto,
            ErrorNegocio::DiaEspecialNoEncontrado => Categoria::NoEncontrado,
            ErrorNegocio::Prohibido => Categoria::Prohibido,
            ErrorNegocio::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorNegocio::DatosInvalidos(_) => "validacion",
            ErrorNegocio::FechaOcupada(_) => "fecha_ocupada",
            ErrorNegocio::DiaEspecialNoEncontrado => "no_encontrado",
            ErrorNegocio::Prohibido => "prohibido",
            ErrorNegocio::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorNegocio::DatosInvalidos(errores) => errores.clone(),
            _ => Vec::new(),
        }
    }
}
