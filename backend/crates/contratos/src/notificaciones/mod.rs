//! Eventos que publica el módulo `notificaciones`.

use chrono::{DateTime, Utc};

use crate::evento;

/// Respuesta de un cliente a un botón de una plantilla de WhatsApp (p. ej. «Confirmo»). Se
/// publica al procesar el webhook de Meta, en la misma transacción que lo marca procesado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespuestaWhatsAppRecibida {
    /// Id del mensaje propio (`notificaciones.mensaje`) al que responde el cliente.
    pub mensaje: i64,
    /// Id del mensaje en Meta (wamid), para los registros.
    pub id_proveedor: String,
    /// Carga útil del botón definida en la plantilla: `confirmar` o `cancelar`.
    pub carga_boton: String,
    pub recibida_en: DateTime<Utc>,
}

evento!(
    RespuestaWhatsAppRecibida,
    "notificaciones.respuesta_whatsapp_recibida"
);
