use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

use crate::dominio::ErrorDominio;

/// Errores de los casos de uso del módulo `identidad`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorIdentidad {
    /// Mensaje genérico: no revela si el correo existe, si la cuenta está bloqueada o si la
    /// contraseña era la incorrecta.
    #[error(
        "Correo o contraseña incorrectos. Después de 5 intentos fallidos la cuenta se bloquea 15 minutos."
    )]
    CredencialesInvalidas,
    #[error("Tu acceso al panel está suspendido. Habla con la administración.")]
    AccesoSuspendido,
    #[error("Inicia sesión para continuar.")]
    NoAutenticado,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("El administrador no existe.")]
    AdministradorNoEncontrado,
    #[error("El enlace venció o ya se usó. Pide uno nuevo.")]
    EnlaceVencido,
    #[error("Ya existe una cuenta con ese correo.")]
    CorreoEnUso,
    #[error("Debe quedar al menos un administrador activo.")]
    UltimoAdministrador,
    #[error("No puedes desactivar tu propia cuenta.")]
    NoPuedeDesactivarse,
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
    #[error("Error interno: {0}")]
    Interno(String),
}

impl ErrorIdentidad {
    pub(crate) fn campo(campo: &str, error: impl ToString) -> Self {
        ErrorIdentidad::DatosInvalidos(vec![ErrorDeCampo::nuevo(campo, error.to_string())])
    }

    pub(crate) fn campo_dominio(campo: &str, error: ErrorDominio) -> Self {
        Self::campo(campo, error)
    }
}

impl From<plataforma::seguridad::ErrorSeguridad> for ErrorIdentidad {
    fn from(error: plataforma::seguridad::ErrorSeguridad) -> Self {
        ErrorIdentidad::Interno(error.0)
    }
}

impl ErrorDeAplicacion for ErrorIdentidad {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorIdentidad::CredencialesInvalidas | ErrorIdentidad::NoAutenticado => Categoria::NoAutenticado,
            ErrorIdentidad::AccesoSuspendido | ErrorIdentidad::Prohibido => Categoria::Prohibido,
            ErrorIdentidad::AdministradorNoEncontrado => Categoria::NoEncontrado,
            ErrorIdentidad::EnlaceVencido => Categoria::Vencido,
            ErrorIdentidad::CorreoEnUso
            | ErrorIdentidad::UltimoAdministrador
            | ErrorIdentidad::NoPuedeDesactivarse => Categoria::Conflicto,
            ErrorIdentidad::DatosInvalidos(_) => Categoria::Validacion,
            ErrorIdentidad::Persistencia(_) | ErrorIdentidad::Interno(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorIdentidad::CredencialesInvalidas => "credenciales_invalidas",
            ErrorIdentidad::AccesoSuspendido => "acceso_suspendido",
            ErrorIdentidad::NoAutenticado => "no_autenticado",
            ErrorIdentidad::Prohibido => "prohibido",
            ErrorIdentidad::AdministradorNoEncontrado => "no_encontrado",
            ErrorIdentidad::EnlaceVencido => "enlace_vencido",
            ErrorIdentidad::CorreoEnUso => "correo_en_uso",
            ErrorIdentidad::UltimoAdministrador => "ultimo_administrador",
            ErrorIdentidad::NoPuedeDesactivarse => "no_puede_desactivarse",
            ErrorIdentidad::DatosInvalidos(_) => "validacion",
            ErrorIdentidad::Persistencia(_) | ErrorIdentidad::Interno(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorIdentidad::DatosInvalidos(errores) => errores.clone(),
            _ => Vec::new(),
        }
    }
}
