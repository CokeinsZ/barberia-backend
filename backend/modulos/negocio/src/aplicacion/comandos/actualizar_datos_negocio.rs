use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::DatosNegocioDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::RepositorioNegocio;
use crate::dominio::entidades::{DatosNegocio, PropuestaDatosNegocio};

/// Actualiza los datos de la barbería y la comisión por defecto (RF-CFG-01). Solo administradores.
#[derive(Debug)]
pub(crate) struct ActualizarDatosNegocio {
    negocio: Arc<dyn RepositorioNegocio>,
}

impl ActualizarDatosNegocio {
    pub(crate) fn nuevo(negocio: Arc<dyn RepositorioNegocio>) -> Self {
        Self { negocio }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        propuesta: PropuestaDatosNegocio,
    ) -> Result<DatosNegocioDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let datos = DatosNegocio::nuevo(propuesta)?;
        self.negocio.guardar(&datos).await?;
        Ok(DatosNegocioDto::from(&datos))
    }
}
