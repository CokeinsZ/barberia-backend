mod activar_cuenta;
mod cambiar_contrasena;
mod cambiar_estado_administrador;
mod cerrar_sesion;
mod iniciar_sesion;
mod invitar_administrador;
mod restablecer_contrasena;
mod solicitar_recuperacion;

pub(crate) use activar_cuenta::ActivarCuenta;
pub(crate) use cambiar_contrasena::CambiarContrasena;
pub(crate) use cambiar_estado_administrador::CambiarEstadoAdministrador;
pub(crate) use cerrar_sesion::CerrarSesion;
pub(crate) use iniciar_sesion::{IniciarSesion, SolicitudInicioSesion};
pub(crate) use invitar_administrador::InvitarAdministrador;
pub(crate) use restablecer_contrasena::RestablecerContrasena;
pub(crate) use solicitar_recuperacion::SolicitarRecuperacion;

/// Módulo y entidad con que este módulo firma sus registros de auditoría.
pub(crate) const MODULO: &str = "identidad";
pub(crate) const ENTIDAD_USUARIO: &str = "usuario";
