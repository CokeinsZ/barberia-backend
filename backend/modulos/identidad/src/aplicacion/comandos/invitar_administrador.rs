use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use notificaciones::api::{CorreoSaliente, PlantillaCorreo};
use nucleo::{Actor, Correo, ErrorDeCampo, ErrorPersistencia};
use plataforma::seguridad::generar_token;
use serde_json::json;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::dto::InvitacionEmitida;
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::politicas::administrador_o_sistema;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::{NombrePersona, PropositoToken};

/// RF-CFG-06: invitar a un administrador. Crea la cuenta sin contraseña y le envía el enlace
/// para crearla (válido 7 días).
#[derive(Debug)]
pub(crate) struct InvitarAdministrador<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> InvitarAdministrador<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        correo: &str,
        nombre: &str,
    ) -> Result<InvitacionEmitida, ErrorIdentidad> {
        let p = &self.p;
        administrador_o_sistema(actor)?;
        let correo = Correo::nuevo(correo);
        let nombre = NombrePersona::nuevo(nombre);
        let (correo, nombre) = match (correo, nombre) {
            (Ok(c), Ok(n)) => (c, n),
            (c, n) => {
                let mut errores = Vec::new();
                if let Err(e) = c {
                    errores.push(ErrorDeCampo::nuevo("correo", e.to_string()));
                }
                if let Err(e) = n {
                    errores.push(ErrorDeCampo::nuevo("nombre", e.to_string()));
                }
                return Err(ErrorIdentidad::DatosInvalidos(errores));
            }
        };

        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let id = p
            .usuarios
            .crear_administrador(&mut cx, &correo, &nombre, ahora)
            .await
            .map_err(|e| match e {
                ErrorPersistencia::Duplicado { .. } => ErrorIdentidad::CorreoEnUso,
                otro => otro.into(),
            })?;
        let (token, huella) = generar_token()?;
        let proposito = PropositoToken::Activacion;
        p.tokens.emitir(&mut cx, id, proposito, &huella, ahora).await?;
        let enlace = proposito.enlace(&p.url_publica, &token);
        p.buzon
            .encolar_correo(
                &mut cx,
                CorreoSaliente {
                    destinatario: correo.clone(),
                    plantilla: PlantillaCorreo::InvitacionPersonal {
                        nombre: nombre.como_str().to_owned(),
                        rol: "administrador".into(),
                        enlace: enlace.clone(),
                    },
                },
            )
            .await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "invitar_administrador", ENTIDAD_USUARIO, id)
                    .despues(&json!({ "correo": correo, "nombre": nombre.como_str() })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(InvitacionEmitida { usuario: id, enlace })
    }
}
