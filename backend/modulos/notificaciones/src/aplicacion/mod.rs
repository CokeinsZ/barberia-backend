//! Casos de uso del worker: envío de la cola y procesamiento de webhooks.

mod enviar_correos;
mod enviar_whatsapp;
pub(crate) mod puertos;
mod webhooks;

#[cfg(test)]
pub(crate) mod pruebas;

pub(crate) use enviar_correos::EnviarCorreosPendientes;
pub(crate) use enviar_whatsapp::EnviarWhatsAppPendientes;
pub(crate) use webhooks::ProcesarWebhooks;
