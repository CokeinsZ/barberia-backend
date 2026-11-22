//! Rutas (bajo `/api/v1`): dashboard, tendencias, alertas, resumen, clientes y barberos, en JSON
//! (vista previa) y en PDF (`.pdf`).

use std::sync::Arc;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use nucleo::identificadores::IdBarbero;
use plataforma::http::{ActorAutenticado, ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::casos::{ArchivoPdf, Reportes, SolicitudPdf};
use crate::aplicacion::errores::ErrorReportes;
use crate::dominio::periodo::Periodo;

type Estado<C> = State<Arc<Reportes<C>>>;

pub(crate) fn rutas<C: Send + 'static>(reportes: Arc<Reportes<C>>) -> Router {
    Router::new()
        .route("/reportes/dashboard", get(dashboard::<C>))
        .route("/reportes/tendencias", get(tendencias::<C>))
        .route("/reportes/alertas", get(alertas::<C>))
        .route("/reportes/resumen", get(resumen::<C>))
        .route("/reportes/resumen.pdf", get(resumen_pdf::<C>))
        .route("/reportes/clientes", get(clientes::<C>))
        .route("/reportes/clientes.pdf", get(clientes_pdf::<C>))
        .route("/reportes/barberos.pdf", get(barberos_pdf::<C>))
        .route("/reportes/barberos/{archivo}", get(barbero::<C>))
        .with_state(reportes)
}

/// `?mes=AAAA-MM` o `?desde=&hasta=`; sin nada, el mes en curso hasta hoy (RF-REP-01).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParametrosPeriodo {
    mes: Option<String>,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParametroFecha {
    fecha: Option<NaiveDate>,
}

/// El 503 del generador de PDF se puede reintentar; lo demás sigue la categoría del error.
fn error(e: ErrorReportes) -> ErrorHttp {
    match e {
        ErrorReportes::PdfNoDisponible => ErrorHttp::nuevo(
            StatusCode::SERVICE_UNAVAILABLE,
            "pdf_no_disponible",
            e.to_string(),
        ),
        otro => ErrorHttp::from(otro),
    }
}

fn periodo<C: Send + 'static>(r: &Reportes<C>, p: &ParametrosPeriodo) -> Result<Periodo, ErrorHttp> {
    r.periodo(p.mes.as_deref(), p.desde, p.hasta).map_err(error)
}

fn descarga(archivo: ArchivoPdf) -> Response {
    let mut respuesta = archivo.contenido.into_response();
    let cabeceras = respuesta.headers_mut();
    cabeceras.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    cabeceras.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    if let Ok(valor) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", archivo.nombre)) {
        cabeceras.insert(header::CONTENT_DISPOSITION, valor);
    }
    respuesta
}

async fn dashboard<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametroFecha>,
) -> Result<Response, ErrorHttp> {
    let fecha = p.fecha.unwrap_or_else(|| r.hoy());
    Ok(Json(r.dashboard(&actor, fecha).await.map_err(error)?).into_response())
}

async fn tendencias<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(Json(r.tendencias(&actor, periodo).await.map_err(error)?).into_response())
}

async fn alertas<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Response, ErrorHttp> {
    Ok(Json(r.alertas(&actor).await.map_err(error)?).into_response())
}

async fn resumen<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(Json(r.resumen(&actor, periodo).await.map_err(error)?).into_response())
}

async fn resumen_pdf<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(descarga(
        r.pdf(&actor, SolicitudPdf::Resumen(periodo))
            .await
            .map_err(error)?,
    ))
}

async fn clientes<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(Json(r.clientes(&actor, periodo).await.map_err(error)?).into_response())
}

async fn clientes_pdf<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(descarga(
        r.pdf(&actor, SolicitudPdf::Clientes(periodo))
            .await
            .map_err(error)?,
    ))
}

async fn barberos_pdf<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let periodo = periodo(&r, &p)?;
    Ok(descarga(
        r.pdf(&actor, SolicitudPdf::Barberos(periodo))
            .await
            .map_err(error)?,
    ))
}

/// `/reportes/barberos/{id}` (vista previa) o `/reportes/barberos/{id}.pdf` (descarga). El
/// barbero solo puede pedir el suyo («Mi reporte», RF-PBA-06).
async fn barbero<C: Send + 'static>(
    State(r): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<String>, PathRejection>,
    Parametros(p): Parametros<ParametrosPeriodo>,
) -> Result<Response, ErrorHttp> {
    let Ok(Path(archivo)) = ruta else {
        return Err(ErrorHttp::solicitud_invalida("Ruta inválida."));
    };
    let (texto, es_pdf) = match archivo.strip_suffix(".pdf") {
        Some(id) => (id, true),
        None => (archivo.as_str(), false),
    };
    let id = texto
        .parse::<i64>()
        .map(IdBarbero)
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    let periodo = periodo(&r, &p)?;
    if es_pdf {
        Ok(descarga(
            r.pdf(&actor, SolicitudPdf::Barbero(id, periodo))
                .await
                .map_err(error)?,
        ))
    } else {
        Ok(Json(r.barbero(&actor, id, periodo).await.map_err(error)?).into_response())
    }
}
