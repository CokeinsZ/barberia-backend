//! `GET /api/v1/auditoria` (solo administradores).

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use nucleo::{Pagina, SolicitudPagina};
use plataforma::base_datos::UnidadDeTrabajo;
use plataforma::http::{ActorAutenticado, ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::dto::RegistroAuditoriaDto;
use crate::aplicacion::{FiltrosAuditoria, ListarRegistros};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltrosSolicitud {
    entidad: Option<String>,
    entidad_id: Option<String>,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

type Estado = State<Arc<ListarRegistros<UnidadDeTrabajo>>>;

pub(crate) fn rutas(listar: Arc<ListarRegistros<UnidadDeTrabajo>>) -> Router {
    Router::new()
        .route("/auditoria", get(listar_registros))
        .with_state(listar)
}

async fn listar_registros(
    State(listar): Estado,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(f): Parametros<FiltrosSolicitud>,
) -> Result<Json<Pagina<RegistroAuditoriaDto>>, ErrorHttp> {
    let defecto = SolicitudPagina::default();
    let filtros = FiltrosAuditoria {
        entidad: f.entidad,
        entidad_id: f.entidad_id,
        desde: f.desde,
        hasta: f.hasta,
        pagina: SolicitudPagina {
            pagina: f.pagina.unwrap_or(defecto.pagina),
            por_pagina: f.por_pagina.unwrap_or(defecto.por_pagina),
        },
    };
    Ok(Json(listar.ejecutar(&actor, filtros).await?))
}
