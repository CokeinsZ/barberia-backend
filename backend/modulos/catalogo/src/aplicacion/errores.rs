use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

/// Errores de los casos de uso del módulo `catalogo`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorCatalogo {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Ya hay un servicio con ese nombre.")]
    NombreEnUso,
    #[error("El servicio no existe.")]
    ServicioNoEncontrado,
    #[error("Solo los combos tienen componentes.")]
    NoEsCombo,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl ErrorCatalogo {
    pub(crate) fn campo(campo: &str, mensaje: impl Into<String>) -> Self {
        ErrorCatalogo::DatosInvalidos(vec![ErrorDeCampo::nuevo(campo, mensaje)])
    }
}

impl From<Vec<ErrorDeCampo>> for ErrorCatalogo {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorCatalogo::DatosInvalidos(errores)
    }
}

impl ErrorDeAplicacion for ErrorCatalogo {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorCatalogo::DatosInvalidos(_) => Categoria::Validacion,
            ErrorCatalogo::NombreEnUso | ErrorCatalogo::NoEsCombo => Categoria::Conflicto,
            ErrorCatalogo::ServicioNoEncontrado => Categoria::NoEncontrado,
            ErrorCatalogo::Prohibido => Categoria::Prohibido,
            ErrorCatalogo::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorCatalogo::DatosInvalidos(_) => "validacion",
            ErrorCatalogo::NombreEnUso => "nombre_en_uso",
            ErrorCatalogo::NoEsCombo => "no_es_combo",
            ErrorCatalogo::ServicioNoEncontrado => "no_encontrado",
            ErrorCatalogo::Prohibido => "prohibido",
            ErrorCatalogo::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorCatalogo::DatosInvalidos(errores) => errores.clone(),
            _ => Vec::new(),
        }
    }
}
