use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::{Actor, Correo};
use plataforma::seguridad::generar_token;
use serde_json::json;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::PropositoToken;

/// Enlace de acceso emitido por línea de comandos (`diamante enlace-acceso`), sin correo: para
/// quien perdió su invitación o no la recibió. Si la cuenta no se ha activado, el enlace es de
/// activación (7 días); si ya se activó, de restablecimiento de contraseña (1 hora). Solo lo
/// puede pedir quien tiene acceso al servidor; queda en la auditoría.
#[derive(Debug)]
pub(crate) struct EmitirEnlaceDeAcceso<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> EmitirEnlaceDeAcceso<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, correo: &str) -> Result<String, ErrorIdentidad> {
        let p = &self.p;
        let correo = Correo::nuevo(correo).map_err(|e| ErrorIdentidad::campo("correo", e))?;
        let mut cx = p.transacciones.iniciar().await?;
        let usuario = p
            .usuarios
            .por_correo(&mut cx, &correo)
            .await?
            .ok_or(ErrorIdentidad::CuentaNoEncontrada)?;
        let identificacion = p.identificar(&mut cx, &usuario).await?;
        if !usuario.activo || identificacion.actor.is_none() {
            return Err(ErrorIdentidad::AccesoSuspendido);
        }
        let proposito = if usuario.esta_activada() {
            PropositoToken::Recuperacion
        } else {
            PropositoToken::Activacion
        };
        let (token, huella) = generar_token()?;
        p.tokens
            .emitir(&mut cx, usuario.id, proposito, &huella, p.reloj.ahora())
            .await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(
                    &Actor::Sistema,
                    MODULO,
                    "emitir_enlace_acceso",
                    ENTIDAD_USUARIO,
                    usuario.id,
                )
                .despues(&json!({ "activacion": proposito == PropositoToken::Activacion })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(proposito.enlace(&p.url_publica, &token))
    }
}
