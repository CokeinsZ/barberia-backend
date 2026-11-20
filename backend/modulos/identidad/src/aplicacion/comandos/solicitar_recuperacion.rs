use std::sync::Arc;

use notificaciones::api::{CorreoSaliente, PlantillaCorreo};
use nucleo::Correo;
use plataforma::seguridad::generar_token;

use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::PropositoToken;

/// RF-AUT-03. La respuesta es la misma exista o no la cuenta; solo las cuentas activadas y
/// con acceso reciben el correo.
#[derive(Debug)]
pub(crate) struct SolicitarRecuperacion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> SolicitarRecuperacion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, correo: &str) -> Result<(), ErrorIdentidad> {
        let p = &self.p;
        let correo = Correo::nuevo(correo).map_err(|e| ErrorIdentidad::campo("correo", e))?;
        let mut cx = p.transacciones.iniciar().await?;
        let Some(usuario) = p.usuarios.por_correo(&mut cx, &correo).await? else {
            return Ok(());
        };
        let identificacion = p.identificar(&mut cx, &usuario).await?;
        if !usuario.activo || !usuario.esta_activada() || identificacion.actor.is_none() {
            return Ok(());
        }

        let (token, huella) = generar_token()?;
        let proposito = PropositoToken::Recuperacion;
        p.tokens
            .emitir(&mut cx, usuario.id, proposito, &huella, p.reloj.ahora())
            .await?;
        p.buzon
            .encolar_correo(
                &mut cx,
                CorreoSaliente {
                    destinatario: usuario.correo.clone(),
                    plantilla: PlantillaCorreo::RecuperacionContrasena {
                        nombre: identificacion.nombre,
                        enlace: proposito.enlace(&p.url_publica, &token),
                    },
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
