use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdServicio;
use serde_json::json;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::Puertos;
use crate::aplicacion::{ENTIDAD, MODULO};

/// Activa o desactiva un servicio (RF-SER-02). Uno inactivo no se agenda ni se vende, pero su
/// historial se conserva.
#[derive(Debug)]
pub(crate) struct CambiarEstadoServicio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> CambiarEstadoServicio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdServicio,
        activo: bool,
    ) -> Result<ServicioDto, ErrorCatalogo> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes = p.dto(&mut cx, id).await?;
        if antes.activo == activo {
            p.transacciones.confirmar(cx).await?;
            return Ok(antes);
        }
        p.servicios.cambiar_estado(&mut cx, id, activo).await?;
        let accion = if activo {
            "activar_servicio"
        } else {
            "desactivar_servicio"
        };
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, accion, ENTIDAD, id)
                    .antes(&json!({ "activo": antes.activo }))
                    .despues(&json!({ "activo": activo })),
            )
            .await?;
        let dto = p.dto(&mut cx, id).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
