use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::DatosNegocioDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::RepositorioNegocio;

/// Consulta los datos de la barbería (RF-CFG-01). Solo administradores.
#[derive(Debug)]
pub(crate) struct ObtenerDatosNegocio {
    negocio: Arc<dyn RepositorioNegocio>,
}

impl ObtenerDatosNegocio {
    pub(crate) fn nuevo(negocio: Arc<dyn RepositorioNegocio>) -> Self {
        Self { negocio }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<DatosNegocioDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let datos = self.negocio.obtener().await?;
        Ok(DatosNegocioDto::from(&datos))
    }
}
