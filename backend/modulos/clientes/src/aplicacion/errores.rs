use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

#[derive(Debug, thiserror::Error)]
pub enum ErrorClientes {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Ya hay un cliente con ese celular.")]
    CelularEnUso,
    #[error("El cliente no existe.")]
    NoEncontrado,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl From<Vec<ErrorDeCampo>> for ErrorClientes {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorClientes::DatosInvalidos(errores)
    }
}

impl ErrorDeAplicacion for ErrorClientes {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorClientes::DatosInvalidos(_) => Categoria::Validacion,
            ErrorClientes::CelularEnUso => Categoria::Conflicto,
            ErrorClientes::NoEncontrado => Categoria::NoEncontrado,
            ErrorClientes::Prohibido => Categoria::Prohibido,
            ErrorClientes::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorClientes::DatosInvalidos(_) => "validacion",
            ErrorClientes::CelularEnUso => "celular_en_uso",
            ErrorClientes::NoEncontrado => "no_encontrado",
            ErrorClientes::Prohibido => "prohibido",
            ErrorClientes::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorClientes::DatosInvalidos(e) => e.clone(),
            _ => Vec::new(),
        }
    }
}
