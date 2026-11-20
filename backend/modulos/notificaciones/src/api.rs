//! Fachada pública del módulo `notificaciones`.

use std::fmt;

use async_trait::async_trait;
use nucleo::{Correo, ErrorPersistencia};
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

/// Encola mensajes dentro del contexto transaccional `C` de quien llama.
#[async_trait]
pub trait BuzonSalida<C: Send>: Send + Sync + fmt::Debug {
    async fn encolar_correo(&self, contexto: &mut C, correo: CorreoSaliente)
    -> Result<(), ErrorPersistencia>;
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
