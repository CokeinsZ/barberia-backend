use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

/// Errores de los casos de uso del módulo `ventas`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorVentas {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Esta cita ya se cobró.")]
    CitaYaCobrada,
    #[error("{0}")]
    EstadoInvalido(&'static str),
    #[error("La venta no existe.")]
    VentaNoEncontrada,
    #[error("Ya se hizo el cierre de caja de ese día.")]
    CierreExistente,
    #[error("No hay cierre de caja para ese día.")]
    CierreNoEncontrado,
    #[error("El método de pago no existe.")]
    MetodoNoEncontrado,
    #[error("No tienes permiso para hacer esto.")]
    Prohibido,
    #[error("{0}")]
    Externo(Box<dyn ErrorDeAplicacion>),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl ErrorVentas {
    pub(crate) fn campo(campo: &str, mensaje: impl Into<String>) -> Self {
        ErrorVentas::DatosInvalidos(vec![ErrorDeCampo::nuevo(campo, mensaje)])
    }
}

impl From<Vec<ErrorDeCampo>> for ErrorVentas {
    fn from(errores: Vec<ErrorDeCampo>) -> Self {
        ErrorVentas::DatosInvalidos(errores)
    }
}

impl From<clientes::api::ErrorClientes> for ErrorVentas {
    fn from(error: clientes::api::ErrorClientes) -> Self {
        ErrorVentas::Externo(Box::new(error))
    }
}

impl From<catalogo::api::ErrorCatalogo> for ErrorVentas {
    fn from(error: catalogo::api::ErrorCatalogo) -> Self {
        ErrorVentas::Externo(Box::new(error))
    }
}

impl From<nucleo::eventos::ErrorDePublicacion> for ErrorVentas {
    fn from(error: nucleo::eventos::ErrorDePublicacion) -> Self {
        // Un manejador rechazó la venta (p. ej. la cita dejó de estar por atender).
        tracing::warn!(error = %error, "la venta no se pudo completar");
        ErrorVentas::EstadoInvalido("La cita ya no está por atender.")
    }
}

impl ErrorDeAplicacion for ErrorVentas {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorVentas::DatosInvalidos(_) => Categoria::Validacion,
            ErrorVentas::CitaYaCobrada | ErrorVentas::EstadoInvalido(_) | ErrorVentas::CierreExistente => {
                Categoria::Conflicto
            }
            ErrorVentas::VentaNoEncontrada
            | ErrorVentas::CierreNoEncontrado
            | ErrorVentas::MetodoNoEncontrado => Categoria::NoEncontrado,
            ErrorVentas::Prohibido => Categoria::Prohibido,
            ErrorVentas::Externo(e) => e.categoria(),
            ErrorVentas::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorVentas::DatosInvalidos(_) => "validacion",
            ErrorVentas::CitaYaCobrada => "cita_ya_cobrada",
            ErrorVentas::EstadoInvalido(_) => "estado_invalido",
            ErrorVentas::CierreExistente => "cierre_existente",
            ErrorVentas::VentaNoEncontrada
            | ErrorVentas::CierreNoEncontrado
            | ErrorVentas::MetodoNoEncontrado => "no_encontrado",
            ErrorVentas::Prohibido => "prohibido",
            ErrorVentas::Externo(e) => e.codigo(),
            ErrorVentas::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorVentas::DatosInvalidos(e) => e.clone(),
            ErrorVentas::Externo(e) => e.errores_de_campo(),
            _ => Vec::new(),
        }
    }
}
