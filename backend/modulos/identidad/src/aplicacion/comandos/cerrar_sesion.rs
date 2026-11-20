use std::sync::Arc;

use plataforma::seguridad::huella_token;

use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;

/// RF-AUT-07. Sin token no hay nada que cerrar y no es un error.
#[derive(Debug)]
pub(crate) struct CerrarSesion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> CerrarSesion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, token: Option<&str>) -> Result<(), ErrorIdentidad> {
        let Some(token) = token else { return Ok(()) };
        let mut cx = self.p.transacciones.iniciar().await?;
        self.p
            .sesiones
            .revocar(&mut cx, &huella_token(token), self.p.reloj.ahora())
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
