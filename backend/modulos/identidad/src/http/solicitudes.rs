//! Cuerpos de las solicitudes. Las validaciones de negocio las hace la capa de aplicación.

use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct InicioSesionSolicitud {
    pub(crate) correo: String,
    pub(crate) contrasena: String,
    #[serde(default)]
    pub(crate) recordarme: bool,
}

/// Activación y confirmación de la recuperación.
#[derive(Deserialize)]
pub(crate) struct EnlaceSolicitud {
    pub(crate) token: String,
    pub(crate) contrasena: String,
}

#[derive(Deserialize)]
pub(crate) struct RecuperacionSolicitud {
    pub(crate) correo: String,
}

#[derive(Deserialize)]
pub(crate) struct CambioContrasenaSolicitud {
    pub(crate) actual: String,
    pub(crate) nueva: String,
}

#[derive(Deserialize)]
pub(crate) struct NuevoAdministradorSolicitud {
    pub(crate) correo: String,
    pub(crate) nombre: String,
}

#[derive(Deserialize)]
pub(crate) struct EstadoAdministradorSolicitud {
    pub(crate) activo: bool,
}
