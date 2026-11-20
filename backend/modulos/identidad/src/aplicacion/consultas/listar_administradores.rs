use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::AdministradorDto;
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::Puertos;

/// RF-CFG-06.
#[derive(Debug)]
pub(crate) struct ListarAdministradores<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ListarAdministradores<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<Vec<AdministradorDto>, ErrorIdentidad> {
        solo_administrador(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let administradores = self.p.usuarios.listar_administradores(&mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(administradores)
    }
}
