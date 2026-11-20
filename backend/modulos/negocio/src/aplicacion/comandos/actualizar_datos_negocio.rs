use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;

use crate::aplicacion::MODULO;
use crate::aplicacion::dto::DatosNegocioDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::entidades::{DatosNegocio, PropuestaDatosNegocio};

/// Actualiza los datos de la barbería y la comisión por defecto (RF-CFG-01). Solo
/// administradores. Queda auditado con el antes y el después.
#[derive(Debug)]
pub(crate) struct ActualizarDatosNegocio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ActualizarDatosNegocio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        propuesta: PropuestaDatosNegocio,
    ) -> Result<DatosNegocioDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let datos = DatosNegocio::nuevo(propuesta)?;
        let despues = DatosNegocioDto::from(&datos);

        let mut cx = self.p.transacciones.iniciar().await?;
        // Sin datos previos (base recién creada) se audita solo el después.
        let antes = self
            .p
            .negocio
            .obtener(&mut cx)
            .await
            .ok()
            .map(|d| DatosNegocioDto::from(&d));
        self.p.negocio.guardar(&mut cx, &datos).await?;
        let mut entrada =
            EntradaAuditoria::nueva(actor, MODULO, "actualizar_datos", "negocio", 1).despues(&despues);
        if let Some(antes) = &antes {
            entrada = entrada.antes(antes);
        }
        self.p.auditor.registrar(&mut cx, entrada).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
