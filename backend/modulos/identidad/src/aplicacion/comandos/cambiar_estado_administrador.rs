use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdUsuario;
use serde_json::json;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::Rol;

/// RF-CFG-06: activar o desactivar un administrador. Desactivar cierra sus sesiones; nunca
/// queda la barbería sin un administrador activo y nadie se desactiva a sí mismo.
#[derive(Debug)]
pub(crate) struct CambiarEstadoAdministrador<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> CambiarEstadoAdministrador<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdUsuario,
        activo: bool,
    ) -> Result<(), ErrorIdentidad> {
        let p = &self.p;
        solo_administrador(actor)?;
        if !activo && actor.usuario() == Some(id) {
            return Err(ErrorIdentidad::NoPuedeDesactivarse);
        }

        let mut cx = p.transacciones.iniciar().await?;
        let usuario = p
            .usuarios
            .por_id(&mut cx, id)
            .await?
            .filter(|u| u.rol == Rol::Administrador)
            .ok_or(ErrorIdentidad::AdministradorNoEncontrado)?;
        if usuario.activo == activo {
            return Ok(());
        }
        if !activo {
            let activos = p.usuarios.administradores_activos(&mut cx).await?;
            if activos.iter().all(|a| *a == id) {
                return Err(ErrorIdentidad::UltimoAdministrador);
            }
        }

        p.usuarios.cambiar_activo(&mut cx, id, activo).await?;
        if !activo {
            p.sesiones
                .revocar_de_usuario(&mut cx, id, None, p.reloj.ahora())
                .await?;
        }
        let accion = if activo {
            "activar_administrador"
        } else {
            "desactivar_administrador"
        };
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, accion, ENTIDAD_USUARIO, id)
                    .antes(&json!({ "activo": usuario.activo }))
                    .despues(&json!({ "activo": activo })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
