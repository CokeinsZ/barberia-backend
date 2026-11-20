//! Fachada pública del módulo `auditoria`.

use std::fmt;

use async_trait::async_trait;
use nucleo::{Actor, ErrorPersistencia};
use serde::Serialize;
use serde_json::Value;

pub use crate::aplicacion::dto::RegistroAuditoriaDto;
pub use crate::aplicacion::errores::ErrorAuditoria;

/// Una acción para auditar. Nunca debe llevar secretos (contraseñas, tokens, hashes).
#[derive(Debug, Clone, PartialEq)]
pub struct EntradaAuditoria {
    pub actor: Actor,
    /// Módulo que registra, p. ej. `identidad`.
    pub modulo: &'static str,
    /// Acción con el formato `entidad.verbo`, p. ej. `administrador.invitado`.
    pub accion: &'static str,
    pub entidad: &'static str,
    pub entidad_id: String,
    pub antes: Option<Value>,
    pub despues: Option<Value>,
}

impl EntradaAuditoria {
    pub fn nueva(
        actor: &Actor,
        modulo: &'static str,
        accion: &'static str,
        entidad: &'static str,
        entidad_id: impl fmt::Display,
    ) -> Self {
        Self {
            actor: actor.clone(),
            modulo,
            accion,
            entidad,
            entidad_id: entidad_id.to_string(),
            antes: None,
            despues: None,
        }
    }

    pub fn antes(mut self, valor: &impl Serialize) -> Self {
        self.antes = serde_json::to_value(valor).ok();
        self
    }

    pub fn despues(mut self, valor: &impl Serialize) -> Self {
        self.despues = serde_json::to_value(valor).ok();
        self
    }
}

/// Escribe en la auditoría dentro del contexto transaccional `C` de quien la llama.
#[async_trait]
pub trait Auditor<C: Send>: Send + Sync + fmt::Debug {
    async fn registrar(&self, contexto: &mut C, entrada: EntradaAuditoria) -> Result<(), ErrorPersistencia>;
}
