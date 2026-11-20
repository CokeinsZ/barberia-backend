//! Fachada pública del módulo `identidad`.

use std::fmt;

use async_trait::async_trait;
use nucleo::ErrorPersistencia;
use nucleo::identificadores::{IdBarbero, IdUsuario};

pub use crate::aplicacion::dto::{AdministradorDto, InvitacionEmitida, SesionDto, UsuarioSesionDto};
pub use crate::aplicacion::errores::ErrorIdentidad;

/// Datos del barbero dueño de una cuenta. Los aporta el módulo `barberos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfilBarbero {
    pub barbero: IdBarbero,
    pub nombre: String,
    /// `false` si está retirado o suspendido hoy: no puede entrar al panel.
    pub puede_acceder: bool,
}

/// Puerto que implementa el módulo `barberos` (inversión de dependencias: `identidad` no
/// conoce a `barberos`).
#[async_trait]
pub trait PerfilesBarberos<C: Send>: Send + Sync + fmt::Debug {
    async fn por_usuario(
        &self,
        contexto: &mut C,
        usuario: IdUsuario,
    ) -> Result<Option<PerfilBarbero>, ErrorPersistencia>;
}
