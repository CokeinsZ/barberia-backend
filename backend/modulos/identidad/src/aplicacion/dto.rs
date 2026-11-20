use chrono::{DateTime, Utc};
use nucleo::identificadores::IdUsuario;
use serde::Serialize;

/// Persona con sesión, tal como la muestra el panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsuarioSesionDto {
    pub id: IdUsuario,
    pub rol: &'static str,
    pub nombre: String,
    pub correo: String,
}

/// Respuesta de `POST /auth/sesion` y `GET /auth/sesion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SesionDto {
    pub usuario: UsuarioSesionDto,
    /// Ruta del panel del rol.
    pub panel: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdministradorDto {
    pub id: IdUsuario,
    pub correo: String,
    pub nombre: String,
    pub activo: bool,
    /// Invitado que aún no ha creado su contraseña.
    pub pendiente_activacion: bool,
    pub ultimo_acceso_en: Option<DateTime<Utc>>,
}

/// Resultado de una invitación. El enlace **no** se devuelve por HTTP (viaja por correo); la
/// línea de comandos sí lo muestra para el primer administrador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitacionEmitida {
    pub usuario: IdUsuario,
    pub enlace: String,
}
