//! Fachada pública del módulo `notificaciones`.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::{Celular, Correo, ErrorPersistencia};
use serde::{Deserialize, Serialize};

/// Plantillas de correo disponibles. Las variables viajan en la cola como JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "plantilla", content = "variables", rename_all = "snake_case")]
pub enum PlantillaCorreo {
    /// Invitación para crear la contraseña de una cuenta del personal.
    InvitacionPersonal {
        nombre: String,
        rol: String,
        enlace: String,
    },
    /// Enlace para restablecer la contraseña.
    RecuperacionContrasena { nombre: String, enlace: String },
}

impl PlantillaCorreo {
    /// Las dos plantillas llevan enlaces con tokens: sus variables se borran al enviarse.
    pub fn contiene_secreto(&self) -> bool {
        match self {
            PlantillaCorreo::InvitacionPersonal { .. } | PlantillaCorreo::RecuperacionContrasena { .. } => {
                true
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorreoSaliente {
    pub destinatario: Correo,
    pub plantilla: PlantillaCorreo,
}

/// Plantillas de WhatsApp. El nombre de cada variante (en `snake_case`) es el nombre de la
/// plantilla que se registra en Meta; el texto está en `docs/diseno/plantillas-whatsapp.md`.
/// Las variables viajan en la cola como JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "plantilla", content = "variables", rename_all = "snake_case")]
pub enum PlantillaWhatsApp {
    /// Código de 4 dígitos para verificar el celular al reservar (RF-RES-09).
    CodigoVerificacion { codigo: String },
    /// Confirmación de la cita con el enlace de gestión (RF-NOT-01).
    CitaConfirmada {
        nombre: String,
        servicio: String,
        barbero: String,
        fecha: String,
        hora: String,
        enlace: String,
    },
    /// Recordatorio automático (24 h o 2 h antes) o manual, con los botones «Confirmo» y
    /// «Cancelar cita» (RF-NOT-02, RF-NOT-03, RF-NOT-05).
    RecordatorioCita {
        nombre: String,
        servicio: String,
        barbero: String,
        fecha: String,
        hora: String,
    },
    /// Aviso de cambio de fecha, hora o barbero (RF-NOT-04).
    CitaReprogramada {
        nombre: String,
        servicio: String,
        barbero: String,
        fecha: String,
        hora: String,
        enlace: String,
    },
    /// Aviso de cancelación (RF-NOT-04).
    CitaCancelada {
        nombre: String,
        servicio: String,
        fecha: String,
        hora: String,
    },
    /// Pide al cliente elegir otra hora porque el barbero no podrá atenderlo (RF-BAR-14).
    AvisoReprogramacion {
        nombre: String,
        servicio: String,
        fecha: String,
        hora: String,
        enlace: String,
    },
}

impl PlantillaWhatsApp {
    /// Códigos y enlaces de gestión son secretos: sus variables se borran al enviarse.
    pub fn contiene_secreto(&self) -> bool {
        matches!(
            self,
            PlantillaWhatsApp::CodigoVerificacion { .. }
                | PlantillaWhatsApp::CitaConfirmada { .. }
                | PlantillaWhatsApp::CitaReprogramada { .. }
                | PlantillaWhatsApp::AvisoReprogramacion { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatsAppSaliente {
    pub destinatario: Celular,
    pub plantilla: PlantillaWhatsApp,
}

/// Encola mensajes dentro del contexto transaccional `C` de quien llama.
#[async_trait]
pub trait BuzonSalida<C: Send>: Send + Sync + fmt::Debug {
    async fn encolar_correo(&self, contexto: &mut C, correo: CorreoSaliente)
    -> Result<(), ErrorPersistencia>;

    /// Encola un WhatsApp y devuelve el id del mensaje (para relacionarlo con la cita y con
    /// las respuestas del cliente).
    async fn encolar_whatsapp(
        &self,
        contexto: &mut C,
        mensaje: WhatsAppSaliente,
    ) -> Result<i64, ErrorPersistencia>;
}

/// Tipos de aviso interno para la administradora (enum `notificaciones.tipo_aviso`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoAviso {
    CambioHorarioConCitas,
    BloqueoConCitas,
    AusenciaConCitas,
    MensajeFallido,
}

impl TipoAviso {
    pub fn codigo(self) -> &'static str {
        match self {
            TipoAviso::CambioHorarioConCitas => "cambio_horario_con_citas",
            TipoAviso::BloqueoConCitas => "bloqueo_con_citas",
            TipoAviso::AusenciaConCitas => "ausencia_con_citas",
            TipoAviso::MensajeFallido => "mensaje_fallido",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NuevoAviso {
    pub tipo: TipoAviso,
    pub titulo: String,
    pub detalle: Option<String>,
    /// Datos para la pantalla, p. ej. `{ "barbero_id": 3, "cita_ids": [41, 42] }`.
    pub datos: serde_json::Value,
}

/// Avisos internos para la administradora (RF-NOT-07, RF-BAR-14), en la transacción de quien llama.
#[async_trait]
pub trait Avisos<C: Send>: Send + Sync + fmt::Debug {
    async fn crear(&self, contexto: &mut C, aviso: NuevoAviso) -> Result<i64, ErrorPersistencia>;

    /// Cuántos avisos no se han leído (alertas del dashboard, RF-DAS-08).
    async fn no_leidos(&self, contexto: &mut C) -> Result<i64, ErrorPersistencia>;
}

/// Estado de un mensaje de la cola, sin sus variables (pueden ser secretas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EstadoMensaje {
    pub id: i64,
    pub canal: String,
    pub plantilla: String,
    /// `pendiente`, `enviado`, `entregado`, `leido`, `fallido` o `cancelado`.
    pub estado: String,
    pub intentos: i16,
    pub ultimo_error: Option<String>,
    pub creado_en: DateTime<Utc>,
    pub enviado_en: Option<DateTime<Utc>>,
}

/// Lectura del estado de mensajes concretos (p. ej. los de una cita).
#[async_trait]
pub trait ConsultaMensajes<C: Send>: Send + Sync + fmt::Debug {
    async fn estados(&self, contexto: &mut C, ids: &[i64]) -> Result<Vec<EstadoMensaje>, ErrorPersistencia>;
}

/// Resultado de una pasada del worker sobre la cola.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ResumenEnvio {
    pub enviados: usize,
    pub reintentos: usize,
    pub fallidos: usize,
}

/// Un canal de la cola de salida (RNF-OBS-02).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct EstadoCanal {
    /// Si el canal envía: SMTP configurado para el correo; un proveedor para WhatsApp.
    pub configurado: bool,
    /// Mensajes cuya hora de envío ya llegó y siguen en la cola.
    pub pendientes: i64,
    /// Minutos desde la hora programada del pendiente más antiguo.
    pub atraso_min: Option<i64>,
    pub enviados_24h: i64,
    pub fallidos_24h: i64,
    pub fallidos_ultima_hora: i64,
}

/// Estado de la cola de salida y de los webhooks de Meta (RNF-OBS-02).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct EstadoCola {
    pub correo: EstadoCanal,
    pub whatsapp: EstadoCanal,
    pub webhooks_pendientes: i64,
    /// Minutos desde que llegó el webhook pendiente más antiguo.
    pub webhooks_atraso_min: Option<i64>,
    pub webhooks_con_error_24h: i64,
}

/// Resultado de procesar los webhooks recibidos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ResumenWebhooks {
    pub eventos: usize,
    pub estados: usize,
    pub respuestas: usize,
    pub con_error: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorNotificaciones {
    #[error("Configuración de SMTP inválida: {0}")]
    ConfiguracionSmtp(String),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}
