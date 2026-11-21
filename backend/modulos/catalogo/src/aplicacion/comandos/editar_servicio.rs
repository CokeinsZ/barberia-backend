use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdServicio;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::{Puertos, nombre_en_uso};
use crate::aplicacion::{ENTIDAD, MODULO};
use crate::dominio::{DatosServicio, PropuestaServicio};

/// Edita un servicio (RF-SER-01). El tipo no cambia. Un cambio de precio solo afecta citas y
/// ventas nuevas: las existentes guardaron su precio (RF-SER-03).
#[derive(Debug)]
pub(crate) struct EditarServicio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> EditarServicio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdServicio,
        mut propuesta: PropuestaServicio,
    ) -> Result<ServicioDto, ErrorCatalogo> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes = p.dto(&mut cx, id).await?;
        if propuesta.tipo.is_empty() {
            propuesta.tipo = antes.tipo.to_owned();
        } else if propuesta.tipo != antes.tipo {
            return Err(ErrorCatalogo::campo(
                "tipo",
                "El tipo de un servicio no se puede cambiar; crea uno nuevo.",
            ));
        }
        let datos = DatosServicio::nuevo(propuesta)?;
        p.validar_categoria(&mut cx, &datos).await?;
        p.servicios
            .actualizar(&mut cx, id, &datos)
            .await
            .map_err(nombre_en_uso)?;
        let despues = p.dto(&mut cx, id).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "editar_servicio", ENTIDAD, id)
                    .antes(&antes)
                    .despues(&despues),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
