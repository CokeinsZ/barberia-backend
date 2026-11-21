use archivos::api::ErrorArchivos;
use catalogo::api::ErrorCatalogo;
use identidad::api::ErrorIdentidad;
use negocio::api::ErrorNegocio;
use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

/// Errores de los casos de uso del módulo `barberos`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorBarberos {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    /// También para un barbero que pide un recurso ajeno: no se revela que existe.
    #[error("El barbero no existe.")]
    NoEncontrado,
    #[error("{0} no existe.")]
    RecursoNoEncontrado(&'static str),
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("Ya existe una cuenta con ese correo.")]
    CorreoEnUso,
    #[error("Ya hay un barbero con ese documento.")]
    DocumentoEnUso,
    #[error("El barbero está retirado.")]
    BarberoRetirado,
    #[error("Ya hay una ausencia registrada que se cruza con esas fechas.")]
    AusenciaSolapada,
    #[error("Ya hay un bloqueo que se cruza con esas horas.")]
    BloqueoSolapado,
    #[error("Ya está cargada la planilla de ese mes.")]
    PlanillaRepetida,
    #[error(transparent)]
    Archivo(#[from] ErrorArchivos),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
    #[error("Error interno: {0}")]
    Interno(String),
}

impl From<Vec<ErrorDeCampo>> for ErrorBarberos {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorBarberos::DatosInvalidos(errores)
    }
}

impl From<ErrorIdentidad> for ErrorBarberos {
    fn from(error: ErrorIdentidad) -> Self {
        match error {
            ErrorIdentidad::CorreoEnUso => ErrorBarberos::CorreoEnUso,
            ErrorIdentidad::Prohibido => ErrorBarberos::Prohibido,
            ErrorIdentidad::DatosInvalidos(errores) => ErrorBarberos::DatosInvalidos(errores),
            ErrorIdentidad::Persistencia(e) => ErrorBarberos::Persistencia(e),
            otro => ErrorBarberos::Interno(otro.to_string()),
        }
    }
}

impl From<ErrorCatalogo> for ErrorBarberos {
    fn from(error: ErrorCatalogo) -> Self {
        match error {
            ErrorCatalogo::Persistencia(e) => ErrorBarberos::Persistencia(e),
            otro => ErrorBarberos::Interno(otro.to_string()),
        }
    }
}

impl From<ErrorNegocio> for ErrorBarberos {
    fn from(error: ErrorNegocio) -> Self {
        match error {
            ErrorNegocio::Persistencia(e) => ErrorBarberos::Persistencia(e),
            otro => ErrorBarberos::Interno(otro.to_string()),
        }
    }
}

impl ErrorDeAplicacion for ErrorBarberos {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorBarberos::DatosInvalidos(_) => Categoria::Validacion,
            ErrorBarberos::NoEncontrado | ErrorBarberos::RecursoNoEncontrado(_) => Categoria::NoEncontrado,
            ErrorBarberos::Prohibido => Categoria::Prohibido,
            ErrorBarberos::CorreoEnUso
            | ErrorBarberos::DocumentoEnUso
            | ErrorBarberos::BarberoRetirado
            | ErrorBarberos::AusenciaSolapada
            | ErrorBarberos::BloqueoSolapado
            | ErrorBarberos::PlanillaRepetida => Categoria::Conflicto,
            ErrorBarberos::Archivo(e) => e.categoria(),
            ErrorBarberos::Persistencia(_) | ErrorBarberos::Interno(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorBarberos::DatosInvalidos(_) => "validacion",
            ErrorBarberos::NoEncontrado | ErrorBarberos::RecursoNoEncontrado(_) => "no_encontrado",
            ErrorBarberos::Prohibido => "prohibido",
            ErrorBarberos::CorreoEnUso => "correo_en_uso",
            ErrorBarberos::DocumentoEnUso => "documento_en_uso",
            ErrorBarberos::BarberoRetirado => "barbero_retirado",
            ErrorBarberos::AusenciaSolapada => "ausencia_solapada",
            ErrorBarberos::BloqueoSolapado => "bloqueo_solapado",
            ErrorBarberos::PlanillaRepetida => "planilla_repetida",
            ErrorBarberos::Archivo(e) => e.codigo(),
            ErrorBarberos::Persistencia(_) | ErrorBarberos::Interno(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorBarberos::DatosInvalidos(errores) => errores.clone(),
            ErrorBarberos::Archivo(e) => e.errores_de_campo(),
            _ => Vec::new(),
        }
    }
}
