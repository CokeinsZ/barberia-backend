use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::{SesionDto, UsuarioSesionDto};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::politicas::cuenta_propia;
use crate::aplicacion::puertos::Puertos;

/// Datos de la persona con sesión (`GET /auth/sesion`).
#[derive(Debug)]
pub(crate) struct ConsultarSesion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ConsultarSesion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<SesionDto, ErrorIdentidad> {
        let p = &self.p;
        let id = cuenta_propia(actor)?;
        let mut cx = p.transacciones.iniciar().await?;
        let usuario = p
            .usuarios
            .por_id(&mut cx, id)
            .await?
            .ok_or(ErrorIdentidad::NoAutenticado)?;
        let identificacion = p.identificar(&mut cx, &usuario).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(SesionDto {
            usuario: UsuarioSesionDto {
                id,
                rol: usuario.rol.como_str(),
                nombre: identificacion.nombre,
                correo: usuario.correo.to_string(),
            },
            panel: usuario.rol.panel(),
        })
    }
}
