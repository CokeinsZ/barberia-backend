use chrono::{DateTime, FixedOffset};
use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

/// Errores de los casos de uso del módulo `agenda`. Los de otros módulos (verificación,
/// clientes…) se conservan con su categoría y su código.
#[derive(Debug, thiserror::Error)]
pub enum ErrorAgenda {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("La hora ya no está disponible. Elige otra.")]
    ConflictoHorario {
        alternativas: Vec<DateTime<FixedOffset>>,
    },
    #[error("No puedes reservar en línea por inasistencias anteriores. Comunícate con la barbería.")]
    ReservaBloqueada,
    #[error("{0}")]
    EstadoInvalido(&'static str),
    #[error("La cita no existe.")]
    CitaNoEncontrada,
    #[error("El enlace de la cita no es válido o venció.")]
    EnlaceVencido,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("{0}")]
    Externo(Box<dyn ErrorDeAplicacion>),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
    #[error("Error interno: {0}")]
    Interno(String),
}

impl ErrorAgenda {
    pub(crate) fn campo(campo: &str, mensaje: impl Into<String>) -> Self {
        ErrorAgenda::DatosInvalidos(vec![ErrorDeCampo::nuevo(campo, mensaje)])
    }
}

macro_rules! desde_externo {
    ($($tipo:path),+) => {
        $(impl From<$tipo> for ErrorAgenda {
            fn from(error: $tipo) -> Self {
                ErrorAgenda::Externo(Box::new(error))
            }
        })+
    };
}
desde_externo!(
    verificacion::api::ErrorVerificacion,
    clientes::api::ErrorClientes,
    catalogo::api::ErrorCatalogo,
    negocio::api::ErrorNegocio,
    barberos::api::ErrorBarberos
);

impl From<Vec<ErrorDeCampo>> for ErrorAgenda {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorAgenda::DatosInvalidos(errores)
    }
}

impl ErrorDeAplicacion for ErrorAgenda {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorAgenda::DatosInvalidos(_) => Categoria::Validacion,
            ErrorAgenda::ConflictoHorario { .. } | ErrorAgenda::EstadoInvalido(_) => Categoria::Conflicto,
            ErrorAgenda::ReservaBloqueada | ErrorAgenda::Prohibido => Categoria::Prohibido,
            ErrorAgenda::CitaNoEncontrada => Categoria::NoEncontrado,
            ErrorAgenda::EnlaceVencido => Categoria::Vencido,
            ErrorAgenda::Externo(e) => e.categoria(),
            ErrorAgenda::Persistencia(_) | ErrorAgenda::Interno(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorAgenda::DatosInvalidos(_) => "validacion",
            ErrorAgenda::ConflictoHorario { .. } => "conflicto_horario",
            ErrorAgenda::ReservaBloqueada => "reserva_bloqueada",
            ErrorAgenda::EstadoInvalido(_) => "estado_invalido",
            ErrorAgenda::CitaNoEncontrada => "no_encontrado",
            ErrorAgenda::EnlaceVencido => "enlace_vencido",
            ErrorAgenda::Prohibido => "prohibido",
            ErrorAgenda::Externo(e) => e.codigo(),
            ErrorAgenda::Persistencia(_) | ErrorAgenda::Interno(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorAgenda::DatosInvalidos(e) => e.clone(),
            ErrorAgenda::Externo(e) => e.errores_de_campo(),
            _ => Vec::new(),
        }
    }

    fn reintentar_en_segundos(&self) -> Option<u64> {
        match self {
            ErrorAgenda::Externo(e) => e.reintentar_en_segundos(),
            _ => None,
        }
    }
}
