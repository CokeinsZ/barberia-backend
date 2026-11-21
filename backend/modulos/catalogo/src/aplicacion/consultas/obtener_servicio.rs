use std::sync::Arc;

use nucleo::Actor;
use nucleo::identificadores::IdServicio;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_personal;
use crate::aplicacion::puertos::Puertos;

#[derive(Debug)]
pub(crate) struct ObtenerServicio<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ObtenerServicio<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor, id: IdServicio) -> Result<ServicioDto, ErrorCatalogo> {
        solo_personal(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let dto = self.p.dto(&mut cx, id).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
