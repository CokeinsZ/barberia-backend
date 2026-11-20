//! Envío de correo por SMTP con `lettre`.

use std::time::Duration;

use async_trait::async_trait;
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use plataforma::configuracion::{ConfiguracionSmtp, SeguridadSmtp};

use crate::api::ErrorNotificaciones;
use crate::aplicacion::puertos::{ErrorTransporte, TransporteCorreo};
use crate::dominio::plantillas::CorreoRenderizado;

const ESPERA_MAXIMA: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub(crate) struct TransporteSmtp {
    transporte: AsyncSmtpTransport<Tokio1Executor>,
    remitente: Mailbox,
}

impl TransporteSmtp {
    pub(crate) fn nuevo(config: &ConfiguracionSmtp) -> Result<Self, ErrorNotificaciones> {
        let invalida =
            |e: lettre::transport::smtp::Error| ErrorNotificaciones::ConfiguracionSmtp(e.to_string());
        let constructor = match config.seguridad {
            SeguridadSmtp::Ninguna => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host),
            SeguridadSmtp::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host).map_err(invalida)?
            }
            SeguridadSmtp::Tls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host).map_err(invalida)?
            }
        };
        let mut constructor = constructor.port(config.puerto).timeout(Some(ESPERA_MAXIMA));
        if let (Some(usuario), Some(clave)) = (&config.usuario, &config.clave) {
            constructor =
                constructor.credentials(Credentials::new(usuario.clone(), clave.exponer().to_owned()));
        }
        let remitente = config
            .remitente
            .parse::<Mailbox>()
            .map_err(|e| ErrorNotificaciones::ConfiguracionSmtp(format!("remitente inválido: {e}")))?;
        Ok(Self {
            transporte: constructor.build(),
            remitente,
        })
    }
}

#[async_trait]
impl TransporteCorreo for TransporteSmtp {
    async fn enviar(&self, destinatario: &str, correo: &CorreoRenderizado) -> Result<(), ErrorTransporte> {
        let para = destinatario.parse::<Mailbox>().map_err(|e| ErrorTransporte {
            mensaje: format!("destinatario inválido: {e}"),
            permanente: true,
        })?;
        let mensaje = Message::builder()
            .from(self.remitente.clone())
            .to(para)
            .subject(&correo.asunto)
            .multipart(MultiPart::alternative_plain_html(
                correo.texto.clone(),
                correo.html.clone(),
            ))
            .map_err(|e| ErrorTransporte {
                mensaje: e.to_string(),
                permanente: true,
            })?;
        self.transporte
            .send(mensaje)
            .await
            .map(|_| ())
            .map_err(|e| ErrorTransporte {
                mensaje: e.to_string(),
                permanente: e.is_permanent(),
            })
    }
}
