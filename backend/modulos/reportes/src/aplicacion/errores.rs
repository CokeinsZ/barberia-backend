use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

#[derive(Debug, thiserror::Error)]
pub enum ErrorReportes {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("No tienes permiso para ver este reporte.")]
    Prohibido,
    /// Lo ajeno «no existe» para el barbero (RN-26).
    #[error("El barbero no existe.")]
    BarberoNoEncontrado,
    #[error("El generador de PDF no está disponible. Intenta de nuevo en unos minutos.")]
    PdfNoDisponible,
    #[error("{0}")]
    Externo(Box<dyn ErrorDeAplicacion>),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl From<Vec<ErrorDeCampo>> for ErrorReportes {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorReportes::DatosInvalidos(errores)
    }
}

macro_rules! desde_externo {
    ($($tipo:path),+) => {
        $(impl From<$tipo> for ErrorReportes {
            fn from(error: $tipo) -> Self {
                ErrorReportes::Externo(Box::new(error))
            }
        })+
    };
}
desde_externo!(
    negocio::api::ErrorNegocio,
    clientes::api::ErrorClientes,
    barberos::api::ErrorBarberos
);

impl ErrorDeAplicacion for ErrorReportes {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorReportes::DatosInvalidos(_) => Categoria::Validacion,
            ErrorReportes::Prohibido => Categoria::Prohibido,
            ErrorReportes::BarberoNoEncontrado => Categoria::NoEncontrado,
            // 503: se puede reintentar (el adaptador HTTP lo marca con `Retry-After`).
            ErrorReportes::PdfNoDisponible => Categoria::Interno,
            ErrorReportes::Externo(e) => e.categoria(),
            ErrorReportes::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorReportes::DatosInvalidos(_) => "validacion",
            ErrorReportes::Prohibido => "prohibido",
            ErrorReportes::BarberoNoEncontrado => "no_encontrado",
            ErrorReportes::PdfNoDisponible => "pdf_no_disponible",
            ErrorReportes::Externo(e) => e.codigo(),
            ErrorReportes::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorReportes::DatosInvalidos(e) => e.clone(),
            ErrorReportes::Externo(e) => e.errores_de_campo(),
            _ => Vec::new(),
        }
    }
}
