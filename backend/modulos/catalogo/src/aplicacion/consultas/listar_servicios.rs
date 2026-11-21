use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::ServicioDto;
use crate::aplicacion::errores::ErrorCatalogo;
use crate::aplicacion::politicas::solo_personal;
use crate::aplicacion::puertos::Puertos;

/// Servicios y combos con sus componentes y cuántos barberos los ofrecen (RF-SER-01).
#[derive(Debug)]
pub(crate) struct ListarServicios<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ListarServicios<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        incluir_inactivos: bool,
    ) -> Result<Vec<ServicioDto>, ErrorCatalogo> {
        solo_personal(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let servicios = self.p.servicios.listar(&mut cx, incluir_inactivos).await?;
        let oferentes = self.p.oferentes.contar(&mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(servicios
            .into_iter()
            .map(|s| {
                let barberos = oferentes.get(&s.id).copied().unwrap_or_default();
                ServicioDto::desde(s, barberos)
            })
            .collect())
    }
}
