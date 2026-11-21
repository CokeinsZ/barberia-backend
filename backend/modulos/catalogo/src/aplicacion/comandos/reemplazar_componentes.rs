use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdServicio;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::Puertos;
use crate::aplicacion::{ENTIDAD, MODULO};

/// Cambia los servicios que forman un combo (RF-SER-01): dos o más servicios simples activos.
#[derive(Debug)]
pub(crate) struct ReemplazarComponentes<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ReemplazarComponentes<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        combo: IdServicio,
        componentes: Vec<IdServicio>,
    ) -> Result<ServicioDto, ErrorCatalogo> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes = p.dto(&mut cx, combo).await?;
        if antes.tipo != "combo" {
            return Err(ErrorCatalogo::NoEsCombo);
        }
        p.validar_componentes(&mut cx, Some(combo), &componentes).await?;
        p.servicios
            .reemplazar_componentes(&mut cx, combo, &componentes)
            .await?;
        let despues = p.dto(&mut cx, combo).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "cambiar_componentes", ENTIDAD, combo)
                    .antes(&antes.componentes)
                    .despues(&despues.componentes),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
