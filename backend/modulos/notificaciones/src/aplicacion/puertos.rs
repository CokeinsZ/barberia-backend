use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;

use crate::api::PlantillaCorreo;
use crate::dominio::plantillas::CorreoRenderizado;

/// Correo reservado para enviarse. `intentos` ya cuenta el intento actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorreoPendiente {
    pub(crate) id: i64,
    pub(crate) destinatario: String,
    /// `None` si las variables guardadas no corresponden a ninguna plantilla.
    pub(crate) plantilla: Option<PlantillaCorreo>,
    pub(crate) intentos: i16,
}

#[async_trait]
pub(crate) trait RepositorioCorreos: Send + Sync + std::fmt::Debug {
    /// Reserva hasta `limite` correos pendientes y vencidos, y los aparta hasta `reservar_hasta`
    /// para que otro proceso no los tome. Si el worker se cae, vuelven a quedar disponibles.
    async fn reservar(
        &self,
        ahora: DateTime<Utc>,
        reservar_hasta: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<CorreoPendiente>, ErrorPersistencia>;

    async fn marcar_enviado(&self, id: i64, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia>;

    /// Con `reintento = None`, el mensaje queda fallido definitivamente.
    async fn marcar_fallo(
        &self,
        id: i64,
        error: &str,
        reintento: Option<DateTime<Utc>>,
    ) -> Result<(), ErrorPersistencia>;
}

/// Falla de un envío. Las permanentes (dirección inválida, buzón inexistente) no se reintentan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorTransporte {
    pub(crate) mensaje: String,
    pub(crate) permanente: bool,
}

#[async_trait]
pub(crate) trait TransporteCorreo: Send + Sync + std::fmt::Debug {
    async fn enviar(&self, destinatario: &str, correo: &CorreoRenderizado) -> Result<(), ErrorTransporte>;
}
