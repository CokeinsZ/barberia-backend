use std::sync::Arc;

use nucleo::Actor;
use plataforma::seguridad::huella_token;

use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::DuracionSesion;

/// Convierte el token de la cookie en el actor de la petición. Se ejecuta en cada petición
/// autenticada: una sesión revocada, vencida o de una cuenta sin acceso deja de servir de
/// inmediato (RF-AUT-08).
#[derive(Debug)]
pub(crate) struct ValidarSesion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ValidarSesion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, token: &str) -> Result<Option<Actor>, ErrorIdentidad> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let Some(sesion) = p.sesiones.vigente(&mut cx, &huella_token(token), ahora).await? else {
            return Ok(None);
        };
        if !sesion.usuario.activo {
            return Ok(None);
        }
        let Some(actor) = p.identificar(&mut cx, &sesion.usuario).await?.actor else {
            return Ok(None);
        };
        if DuracionSesion::debe_renovarse(sesion.ultimo_uso_en, ahora) {
            p.sesiones
                .renovar(&mut cx, sesion.id, ahora, sesion.duracion.expira_en(ahora))
                .await?;
        }
        p.transacciones.confirmar(cx).await?;
        Ok(Some(actor))
    }
}
