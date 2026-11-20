use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

#[derive(Debug, thiserror::Error)]
pub enum ErrorAuditoria {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl ErrorDeAplicacion for ErrorAuditoria {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorAuditoria::DatosInvalidos(_) => Categoria::Validacion,
            ErrorAuditoria::Prohibido => Categoria::Prohibido,
            ErrorAuditoria::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorAuditoria::DatosInvalidos(_) => "validacion",
            ErrorAuditoria::Prohibido => "prohibido",
            ErrorAuditoria::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorAuditoria::DatosInvalidos(errores) => errores.clone(),
            _ => Vec::new(),
        }
    }
}
