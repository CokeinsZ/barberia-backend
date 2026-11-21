//! Fachada pública del módulo `trabajos`.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, TimeDelta, Utc};
use serde::Serialize;

/// Tarea en segundo plano que otro módulo (o `app`) registra en el worker.
#[async_trait]
pub trait Tarea: Send + Sync + fmt::Debug {
    /// Tipo estable, p. ej. `agenda.recordatorios`.
    fn tipo(&self) -> &'static str;

    /// Ejecuta la tarea y devuelve un resumen para el registro. Debe ser idempotente: si el
    /// worker se cae a mitad de camino, la tarea se vuelve a ejecutar.
    async fn ejecutar(&self, contexto: &ContextoTarea) -> Result<String, ErrorTarea>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextoTarea {
    /// Inicio de la franja para la que se programó (no la hora real de ejecución).
    pub programada_para: DateTime<Utc>,
    pub carga: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ErrorTarea(pub String);

impl ErrorTarea {
    pub fn nuevo(causa: impl fmt::Display) -> Self {
        Self(causa.to_string())
    }
}

/// Cada cuánto se programa una tarea periódica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Periodicidad {
    /// En franjas fijas desde la época Unix (p. ej. cada 5 minutos: 15:00, 15:05…).
    Cada(TimeDelta),
    /// Una vez al día a esa hora de Bogotá (p. ej. el mantenimiento a las 3:00 a. m., fuera
    /// del horario de atención: RNF-DIS-02).
    DiariaA(NaiveTime),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Programacion {
    pub tipo: &'static str,
    pub periodicidad: Periodicidad,
}

/// Resultado de una pasada del worker por la cola de trabajos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ResumenTrabajos {
    pub programados: usize,
    pub completados: usize,
    pub reintentos: usize,
    pub fallidos: usize,
}
