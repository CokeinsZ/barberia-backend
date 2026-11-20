use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use plataforma::seguridad::huella_token;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::{ContrasenaNueva, PropositoToken};

/// RF-AUT-02: crear la contraseña desde el enlace de la invitación.
#[derive(Debug)]
pub(crate) struct ActivarCuenta<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ActivarCuenta<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, token: &str, contrasena: String) -> Result<(), ErrorIdentidad> {
        crear_contrasena_con_enlace(
            &self.p,
            token,
            contrasena,
            PropositoToken::Activacion,
            "activar_cuenta",
        )
        .await
    }
}

/// Activación y recuperación comparten el flujo: validar la contraseña antes de gastar el
/// enlace, consumir el enlace, guardar la contraseña, cerrar todas las sesiones y auditar.
pub(crate) async fn crear_contrasena_con_enlace<C: Send>(
    p: &Puertos<C>,
    token: &str,
    contrasena: String,
    proposito: PropositoToken,
    accion: &'static str,
) -> Result<(), ErrorIdentidad> {
    let contrasena =
        ContrasenaNueva::nueva(contrasena).map_err(|e| ErrorIdentidad::campo_dominio("contrasena", e))?;
    // El cifrado (lento a propósito) va antes de abrir la transacción.
    let hash = p.credenciales.cifrar(&contrasena).await?;
    let ahora = p.reloj.ahora();

    let mut cx = p.transacciones.iniciar().await?;
    let Some(id) = p
        .tokens
        .consumir(&mut cx, &huella_token(token), proposito, ahora)
        .await?
    else {
        return Err(ErrorIdentidad::EnlaceVencido);
    };
    let usuario = p
        .usuarios
        .por_id(&mut cx, id)
        .await?
        .ok_or(ErrorIdentidad::EnlaceVencido)?;
    if !usuario.activo {
        return Err(ErrorIdentidad::EnlaceVencido);
    }
    p.usuarios.guardar_contrasena(&mut cx, id, &hash).await?;
    p.sesiones.revocar_de_usuario(&mut cx, id, None, ahora).await?;
    let actor = p
        .identificar(&mut cx, &usuario)
        .await?
        .actor
        .unwrap_or(Actor::Sistema);
    p.auditor
        .registrar(
            &mut cx,
            EntradaAuditoria::nueva(&actor, MODULO, accion, ENTIDAD_USUARIO, id),
        )
        .await?;
    p.transacciones.confirmar(cx).await?;
    Ok(())
}
