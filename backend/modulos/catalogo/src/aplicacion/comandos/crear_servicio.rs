use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdServicio;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::{Puertos, nombre_en_uso};
use crate::aplicacion::{ENTIDAD, MODULO};
use crate::dominio::{DatosServicio, PropuestaServicio, TipoServicio};

#[derive(Debug, Clone, Default)]
pub(crate) struct NuevoServicio {
    pub(crate) propuesta: PropuestaServicio,
    /// Obligatorio en los combos; prohibido en los simples.
    pub(crate) componentes: Vec<IdServicio>,
}

/// Crea un servicio simple o un combo (RF-SER-01, 04, 05 y 08). Solo administradores.
#[derive(Debug)]
pub(crate) struct CrearServicio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> CrearServicio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        nuevo: NuevoServicio,
    ) -> Result<ServicioDto, ErrorCatalogo> {
        solo_administrador(actor)?;
        let datos = DatosServicio::nuevo(nuevo.propuesta)?;
        if datos.tipo == TipoServicio::Simple && !nuevo.componentes.is_empty() {
            return Err(ErrorCatalogo::campo(
                "componentes",
                "Solo los combos tienen componentes.",
            ));
        }
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.validar_categoria(&mut cx, &datos).await?;
        if datos.tipo == TipoServicio::Combo {
            p.validar_componentes(&mut cx, None, &nuevo.componentes).await?;
        }
        let id = p.servicios.crear(&mut cx, &datos).await.map_err(nombre_en_uso)?;
        if datos.tipo == TipoServicio::Combo {
            p.servicios
                .reemplazar_componentes(&mut cx, id, &nuevo.componentes)
                .await?;
        }
        let dto = p.dto(&mut cx, id).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "crear_servicio", ENTIDAD, id).despues(&dto),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
