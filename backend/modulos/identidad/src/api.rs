//! Fachada pública del módulo `identidad`.

use std::fmt;

use async_trait::async_trait;
use nucleo::identificadores::{IdBarbero, IdUsuario};
use nucleo::{Actor, Correo, ErrorPersistencia};

pub use crate::aplicacion::dto::{
    AdministradorDto, CuentaPersonalDto, InvitacionEmitida, SesionDto, UsuarioSesionDto,
};
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

/// Cuentas del panel para el personal que administra otro módulo (los barberos los registra el
/// módulo `barberos`). Todo ocurre dentro de la transacción de quien llama y queda auditado.
#[async_trait]
pub trait CuentasPersonal<C: Send>: Send + Sync + fmt::Debug {
    /// Crea la cuenta de un barbero. Con `acceso`, le envía la invitación (vence en 7 días).
    /// `CorreoEnUso` si el correo ya es de otra cuenta.
    async fn crear_cuenta_barbero(
        &self,
        cx: &mut C,
        actor: &Actor,
        correo: &Correo,
        nombre: &str,
        acceso: bool,
    ) -> Result<IdUsuario, ErrorIdentidad>;

    async fn cuenta(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
    ) -> Result<Option<CuentaPersonalDto>, ErrorIdentidad>;

    /// Cambia el correo (que es el usuario del panel). Si la cuenta aún no se activó, reenvía la
    /// invitación al correo nuevo.
    async fn cambiar_correo(
        &self,
        cx: &mut C,
        actor: &Actor,
        usuario: IdUsuario,
        correo: &Correo,
        nombre: &str,
    ) -> Result<(), ErrorIdentidad>;

    /// Da o quita el acceso al panel (RF-BAR-09, RF-AUT-08). Al quitarlo se cierran sus sesiones;
    /// al darlo a una cuenta que no ha creado su contraseña, se le envía una invitación nueva.
    async fn cambiar_acceso(
        &self,
        cx: &mut C,
        actor: &Actor,
        usuario: IdUsuario,
        acceso: bool,
        nombre: &str,
    ) -> Result<(), ErrorIdentidad>;
}
