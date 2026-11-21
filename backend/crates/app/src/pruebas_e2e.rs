//! Rutas de apoyo para las pruebas (`/api/v1/_pruebas/*`), solo con `DIAMANTE_PRUEBAS_E2E=true`
//! (la configuración lo prohíbe en producción):
//!
//! - `GET | POST | DELETE /_pruebas/reloj`: consultar, fijar (`{ "ahora": "…" }`) o devolver a
//!   la hora real el reloj del sistema. Desde el instante fijado, el tiempo sigue corriendo.
//! - `POST /_pruebas/trabajos/ejecutar`: una pasada del worker en ese momento (envíos,
//!   webhooks, recordatorios y mantenimiento que estén listos).
//!
//! El reloj vive en la API: para que el worker use la misma hora, el ambiente E2E ejecuta las
//! tareas dentro de la API (`DIAMANTE_WORKER_EN_API=true`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset};
use nucleo::ZONA_HORARIA;
use plataforma::http::{Entrada, ErrorHttp};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::worker::{ResumenPasada, pasada};

pub(crate) fn rutas(composicion: Composicion) -> Router {
    Router::new()
        .route(
            "/_pruebas/reloj",
            get(ver_reloj).post(fijar_reloj).delete(restablecer_reloj),
        )
        .route("/_pruebas/trabajos/ejecutar", post(ejecutar_trabajos))
        .with_state(composicion)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NuevaHora {
    ahora: DateTime<FixedOffset>,
}

fn sin_reloj() -> ErrorHttp {
    ErrorHttp::nuevo(
        StatusCode::CONFLICT,
        "estado_invalido",
        "El reloj de pruebas no está activo.",
    )
}

fn hora(c: &Composicion) -> Value {
    json!({ "ahora": c.reloj.ahora().with_timezone(&ZONA_HORARIA).fixed_offset() })
}

async fn ver_reloj(State(c): State<Composicion>) -> Json<Value> {
    Json(hora(&c))
}

async fn fijar_reloj(
    State(c): State<Composicion>,
    Entrada(n): Entrada<NuevaHora>,
) -> Result<Json<Value>, ErrorHttp> {
    let reloj = c.reloj_de_pruebas.as_ref().ok_or_else(sin_reloj)?;
    reloj.fijar(n.ahora.to_utc());
    tracing::warn!(ahora = %n.ahora, "reloj de pruebas fijado");
    Ok(Json(hora(&c)))
}

async fn restablecer_reloj(State(c): State<Composicion>) -> Result<Json<Value>, ErrorHttp> {
    let reloj = c.reloj_de_pruebas.as_ref().ok_or_else(sin_reloj)?;
    reloj.restablecer();
    Ok(Json(hora(&c)))
}

async fn ejecutar_trabajos(State(c): State<Composicion>) -> Json<ResumenPasada> {
    Json(pasada(&c).await)
}
