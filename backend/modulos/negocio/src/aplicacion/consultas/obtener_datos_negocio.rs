use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::DatosNegocioDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;

/// Consulta los datos de la barbería (RF-CFG-01). Solo administradores.
#[derive(Debug)]
pub(crate) struct ObtenerDatosNegocio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ObtenerDatosNegocio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<DatosNegocioDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let datos = self.p.negocio.obtener(&mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(DatosNegocioDto::from(&datos))
    }
}
