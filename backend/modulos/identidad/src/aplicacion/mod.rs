//! Casos de uso del módulo: un archivo por caso de uso, con su política de autorización.

pub(crate) mod comandos;
pub(crate) mod consultas;
pub(crate) mod dto;
pub(crate) mod errores;
mod fachada_cuentas;
pub(crate) mod politicas;
pub(crate) mod puertos;

#[cfg(test)]
mod pruebas;

use std::sync::Arc;

use comandos::{
    ActivarCuenta, CambiarContrasena, CambiarEstadoAdministrador, CerrarSesion, EmitirEnlaceDeAcceso,
    IniciarSesion, InvitarAdministrador, RestablecerContrasena, SolicitarRecuperacion,
};
use consultas::{ConsultarSesion, ListarAdministradores, ValidarSesion};
use fachada_cuentas::FachadaCuentas;
use puertos::Puertos;

/// Todos los casos de uso del módulo, conectados con los mismos puertos.
#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) iniciar_sesion: IniciarSesion<C>,
    pub(crate) validar_sesion: Arc<ValidarSesion<C>>,
    pub(crate) consultar_sesion: ConsultarSesion<C>,
    pub(crate) cerrar_sesion: CerrarSesion<C>,
    pub(crate) activar_cuenta: ActivarCuenta<C>,
    pub(crate) solicitar_recuperacion: SolicitarRecuperacion<C>,
    pub(crate) restablecer_contrasena: RestablecerContrasena<C>,
    pub(crate) cambiar_contrasena: CambiarContrasena<C>,
    pub(crate) invitar_administrador: InvitarAdministrador<C>,
    pub(crate) emitir_enlace_de_acceso: EmitirEnlaceDeAcceso<C>,
    pub(crate) listar_administradores: ListarAdministradores<C>,
    pub(crate) cambiar_estado_administrador: CambiarEstadoAdministrador<C>,
    pub(crate) cuentas: Arc<FachadaCuentas<C>>,
}

impl<C: Send + 'static> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            iniciar_sesion: IniciarSesion::nuevo(p.clone()),
            validar_sesion: Arc::new(ValidarSesion::nuevo(p.clone())),
            consultar_sesion: ConsultarSesion::nuevo(p.clone()),
            cerrar_sesion: CerrarSesion::nuevo(p.clone()),
            activar_cuenta: ActivarCuenta::nuevo(p.clone()),
            solicitar_recuperacion: SolicitarRecuperacion::nuevo(p.clone()),
            restablecer_contrasena: RestablecerContrasena::nuevo(p.clone()),
            cambiar_contrasena: CambiarContrasena::nuevo(p.clone()),
            invitar_administrador: InvitarAdministrador::nuevo(p.clone()),
            emitir_enlace_de_acceso: EmitirEnlaceDeAcceso::nuevo(p.clone()),
            listar_administradores: ListarAdministradores::nuevo(p.clone()),
            cambiar_estado_administrador: CambiarEstadoAdministrador::nuevo(p.clone()),
            cuentas: Arc::new(FachadaCuentas::nueva(p)),
        }
    }
}
