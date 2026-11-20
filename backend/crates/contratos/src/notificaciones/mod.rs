//! Eventos que publica el módulo `notificaciones`.

use crate::evento;

/// Respuesta de un cliente a un botón de una plantilla de WhatsApp (p. ej. «Confirmo»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespuestaWhatsAppRecibida {
    /// Id del mensaje de Meta al que responde (wamid).
    pub mensaje_original: String,
    /// Carga útil del botón definida en la plantilla (p. ej. `confirmar`).
    pub carga_boton: String,
}
evento!(
    RespuestaWhatsAppRecibida,
    "notificaciones.respuesta_whatsapp_recibida"
);
