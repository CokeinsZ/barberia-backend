//! Fachada pública del módulo `notificaciones`.

use std::fmt;

use async_trait::async_trait;
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

/// Plantillas de WhatsApp (en la fase 4 se registran en Meta con estos nombres). Las variables
/// viajan en la cola como JSON.
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
    /// Aviso de cancelación (RF-NOT-04).
    CitaCancelada {
        nombre: String,
        servicio: String,
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
}

impl PlantillaWhatsApp {
    /// Códigos y enlaces de gestión son secretos: sus variables se borran al enviarse.
    pub fn contiene_secreto(&self) -> bool {
        !matches!(self, PlantillaWhatsApp::CitaCancelada { .. })
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

    /// Encola un WhatsApp y devuelve el id del mensaje (para relacionarlo con la cita). En la
    /// fase 3 los mensajes quedan en la cola: el envío por WhatsApp Cloud API llega en la fase 4.
    async fn encolar_whatsapp(
        &self,
        contexto: &mut C,
        mensaje: WhatsAppSaliente,
    ) -> Result<i64, ErrorPersistencia>;
}

/// Resultado de una pasada del worker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumenEnvio {
    pub enviados: usize,
    pub reintentos: usize,
    pub fallidos: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorNotificaciones {
    #[error("Configuración de SMTP inválida: {0}")]
    ConfiguracionSmtp(String),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}
