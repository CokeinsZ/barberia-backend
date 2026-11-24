use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use serde_json::Value;

use crate::api::NuevoAviso;
use crate::dominio::plantillas::CorreoRenderizado;
use crate::dominio::webhook::EstadoEntrega;
use crate::dominio::whatsapp::WhatsAppRenderizado;

/// Canal de la cola (`notificaciones.canal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Canal {
    Correo,
    WhatsApp,
}

impl Canal {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            Canal::Correo => "correo",
            Canal::WhatsApp => "whatsapp",
        }
    }
}

/// Mensaje reservado para enviarse. `intentos` ya cuenta el intento actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MensajePendiente {
    pub(crate) id: i64,
    pub(crate) destinatario: String,
    pub(crate) plantilla: String,
    pub(crate) variables: Value,
    pub(crate) intentos: i16,
    /// Cuándo se encoló (para no enviar lo que ya venció).
    pub(crate) creado_en: DateTime<Utc>,
}

/// La cola de salida vista por el worker. Cada operación es atómica por sí sola: el envío
/// ocurre fuera de cualquier transacción.
#[async_trait]
pub(crate) trait RepositorioMensajes: Send + Sync + std::fmt::Debug {
    /// Reserva hasta `limite` mensajes pendientes y vencidos del canal, y los aparta hasta
    /// `reservar_hasta` para que otro proceso no los tome. Si el worker se cae, vuelven a quedar
    /// disponibles.
    async fn reservar(
        &self,
        canal: Canal,
        ahora: DateTime<Utc>,
        reservar_hasta: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<MensajePendiente>, ErrorPersistencia>;

    /// Con `conservar_variables = false`, las variables secretas se borran.
    async fn marcar_enviado(
        &self,
        id: i64,
        ahora: DateTime<Utc>,
        id_proveedor: Option<&str>,
        conservar_variables: bool,
    ) -> Result<(), ErrorPersistencia>;

    async fn reintentar(&self, id: i64, error: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia>;

    /// Un mensaje que ya no tiene sentido enviar (venció en la cola) queda «cancelado» con el
    /// motivo y sin sus secretos.
    async fn marcar_vencido(
        &self,
        id: i64,
        motivo: &str,
        conservar_variables: bool,
    ) -> Result<(), ErrorPersistencia>;

    /// Fallo definitivo. Con `aviso`, se registra para la administradora en la misma operación.
    async fn marcar_fallido(
        &self,
        id: i64,
        ahora: DateTime<Utc>,
        error: &str,
        conservar_variables: bool,
        aviso: Option<NuevoAviso>,
    ) -> Result<(), ErrorPersistencia>;
}

/// Falla de un envío. Las permanentes (dirección inválida, número sin WhatsApp) no se reintentan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorTransporte {
    pub(crate) mensaje: String,
    pub(crate) permanente: bool,
}

#[async_trait]
pub(crate) trait TransporteCorreo: Send + Sync + std::fmt::Debug {
    async fn enviar(&self, destinatario: &str, correo: &CorreoRenderizado) -> Result<(), ErrorTransporte>;
}

/// Proveedor de WhatsApp (patrón Adapter): hoy el simulador; con Meta, WhatsApp Cloud API.
#[async_trait]
pub(crate) trait PasarelaWhatsApp: Send + Sync + std::fmt::Debug {
    /// Envía una plantilla y devuelve el id del mensaje en el proveedor (wamid).
    async fn enviar(&self, celular: &str, mensaje: &WhatsAppRenderizado) -> Result<String, ErrorTransporte>;

    /// El simulador conserva las variables: sus mensajes nunca salen del sistema y las pruebas
    /// necesitan leer los códigos. Se decide por destinatario: con `DIAMANTE_WHATSAPP_SOLO_A`,
    /// unos salen por Meta y otros se simulan.
    fn conserva_variables(&self, _celular: &str) -> bool {
        false
    }
}

/// Evento de webhook guardado y pendiente de procesar.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EventoWebhook {
    pub(crate) id: i64,
    pub(crate) carga: Value,
}

/// Mensaje propio al que se refiere un webhook.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MensajeRegistrado {
    pub(crate) id: i64,
    pub(crate) estado: String,
    pub(crate) plantilla: String,
    pub(crate) variables: Value,
    pub(crate) destinatario: String,
}

/// Webhooks pendientes, en la transacción `C` del procesamiento (así el cambio de estado, el
/// aviso y la reacción de la agenda quedan juntos o no quedan).
#[async_trait]
pub(crate) trait RepositorioWebhooks<C: Send>: Send + Sync + std::fmt::Debug {
    /// Toma el siguiente evento sin procesar (`FOR UPDATE SKIP LOCKED`).
    async fn tomar(&self, contexto: &mut C) -> Result<Option<EventoWebhook>, ErrorPersistencia>;

    async fn marcar_procesado(
        &self,
        contexto: &mut C,
        id: i64,
        ahora: DateTime<Utc>,
        error: Option<&str>,
    ) -> Result<(), ErrorPersistencia>;

    /// Busca y bloquea el mensaje con ese id del proveedor.
    async fn mensaje_por_proveedor(
        &self,
        contexto: &mut C,
        id_proveedor: &str,
    ) -> Result<Option<MensajeRegistrado>, ErrorPersistencia>;

    async fn cambiar_estado(
        &self,
        contexto: &mut C,
        id: i64,
        estado: EstadoEntrega,
        error: Option<&str>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
}
