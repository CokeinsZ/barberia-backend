use std::sync::Arc;

use nucleo::Actor;
use nucleo::identificadores::IdDiaEspecial;

use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::RepositorioDiasEspeciales;

/// Quita un día especial (RF-CFG-04). Solo administradores.
#[derive(Debug)]
pub(crate) struct EliminarDiaEspecial {
    dias: Arc<dyn RepositorioDiasEspeciales>,
}

impl EliminarDiaEspecial {
    pub(crate) fn nuevo(dias: Arc<dyn RepositorioDiasEspeciales>) -> Self {
        Self { dias }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor, id: IdDiaEspecial) -> Result<(), ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        if self.dias.eliminar(id).await? {
            Ok(())
        } else {
            Err(ErrorNegocio::DiaEspecialNoEncontrado)
        }
    }
}
