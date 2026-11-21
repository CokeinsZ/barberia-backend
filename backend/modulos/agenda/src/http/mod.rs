//! Rutas HTTP del módulo (relativas a `/api/v1`): sitio público (sección 2.1 del diseño) y agenda
//! del personal (sección 2.8).

mod panel;
mod publico;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use plataforma::http::ErrorHttp;
use plataforma::seguridad::LimitadorIntentos;
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::aplicacion::CasosDeUso;
use crate::aplicacion::errores::ErrorAgenda;

pub(crate) struct Estado<C: Send> {
    pub(crate) casos: Arc<CasosDeUso<C>>,
    /// Reservas en línea: 20 por IP cada hora (RNF-SEG-05).
    pub(crate) reservas_por_ip: Arc<LimitadorIntentos>,
}

impl<C: Send> Clone for Estado<C> {
    fn clone(&self) -> Self {
        Self {
            casos: self.casos.clone(),
            reservas_por_ip: self.reservas_por_ip.clone(),
        }
    }
}

impl<C: Send> Estado<C> {
    pub(crate) fn nuevo(casos: Arc<CasosDeUso<C>>) -> Self {
        Self {
            casos,
            reservas_por_ip: Arc::new(LimitadorIntentos::nuevo(20, Duration::from_secs(60 * 60))),
        }
    }
}

pub(crate) fn rutas<C: Send + 'static>(estado: Estado<C>) -> Router {
    publico::rutas::<C>()
        .merge(panel::rutas::<C>())
        .with_state(estado)
}

/// Un conflicto de horario lleva sus horas alternativas en el *Problem Details*.
pub(crate) fn error_http(error: ErrorAgenda) -> ErrorHttp {
    let alternativas = match &error {
        ErrorAgenda::ConflictoHorario { alternativas } => Some(json!(alternativas)),
        _ => None,
    };
    let http = ErrorHttp::from(error);
    match alternativas {
        Some(valor) => http.con_extension("alternativas", valor),
        None => http,
    }
}

/// Número opcional en la consulta: vacío (`barbero_id=`) es `None` («Cualquier barbero»).
pub(crate) fn numero_opcional<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let texto: Option<String> = Option::deserialize(d)?;
    match texto.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) => t
            .parse()
            .map(Some)
            .map_err(|_| serde::de::Error::custom(format!("«{t}» no es un número"))),
    }
}
