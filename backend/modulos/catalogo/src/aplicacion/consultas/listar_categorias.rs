use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::CategoriaDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_personal;
use crate::aplicacion::puertos::Puertos;

/// Categorías fijas (Cabello, Barba, Combos, Complementos), en su orden.
#[derive(Debug)]
pub(crate) struct ListarCategorias<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ListarCategorias<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<Vec<CategoriaDto>, ErrorCatalogo> {
        solo_personal(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let categorias = self.p.servicios.categorias(&mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(categorias)
    }
}
