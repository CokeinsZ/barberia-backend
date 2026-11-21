//! Rutas públicas (sin sesión): landing, agendamiento y gestión de la cita por enlace.

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleo::identificadores::{IdBarbero, IdServicio};
use plataforma::http::{Entrada, ErrorHttp, IpCliente, Parametros};
use serde::Deserialize;

use super::{Estado, error_http, numero_opcional};
use crate::aplicacion::dto::{CalendarioDto, GestionCitaDto, HorasDelDiaDto, OpcionBarberoDto};
use crate::aplicacion::reserva::SolicitudReserva;
use crate::dominio::disponibilidad::Modo;

pub(crate) fn rutas<C: Send + 'static>() -> Router<Estado<C>> {
    Router::new()
        .route("/publico/landing", get(landing::<C>))
        .route("/publico/servicios", get(servicios::<C>))
        .route("/publico/barberos", get(barberos::<C>))
        .route("/publico/disponibilidad/calendario", get(calendario::<C>))
        .route("/publico/disponibilidad/horas", get(horas::<C>))
        .route("/publico/citas", post(reservar::<C>))
        .route("/publico/citas/gestion/{token}", get(ver::<C>))
        .route("/publico/citas/gestion/{token}/cancelar", post(cancelar::<C>))
        .route(
            "/publico/citas/gestion/{token}/reprogramar",
            post(reprogramar::<C>),
        )
        .route(
            "/publico/citas/gestion/{token}/recordatorios",
            put(recordatorios::<C>),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServicioParametro {
    servicio_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarioParametros {
    servicio_id: i64,
    #[serde(default, deserialize_with = "numero_opcional")]
    barbero_id: Option<i64>,
    mes: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HorasParametros {
    pub(crate) servicio_id: i64,
    #[serde(default, deserialize_with = "numero_opcional")]
    pub(crate) barbero_id: Option<i64>,
    pub(crate) fecha: NaiveDate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClienteReserva {
    nombre: String,
    correo: Option<String>,
    fecha_nacimiento: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReservaSolicitud {
    comprobante: String,
    servicio_id: i64,
    barbero_id: Option<i64>,
    inicio: DateTime<FixedOffset>,
    cliente: ClienteReserva,
    autoriza_tratamiento_datos: bool,
    politica_version_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReprogramacionSolicitud {
    pub(crate) inicio: DateTime<FixedOffset>,
    pub(crate) barbero_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordatoriosSolicitud {
    activos: bool,
}

/// RF-PUB-04: caché corta; los cambios del panel se ven en un minuto.
async fn landing<C: Send + 'static>(State(e): State<Estado<C>>) -> Result<Response, ErrorHttp> {
    let landing = e.casos.publico.landing().await.map_err(error_http)?;
    let mut respuesta = Json(landing).into_response();
    respuesta.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(respuesta)
}

async fn servicios<C: Send + 'static>(
    State(e): State<Estado<C>>,
) -> Result<Json<Vec<catalogo::api::ServicioDto>>, ErrorHttp> {
    Ok(Json(e.casos.publico.servicios().await.map_err(error_http)?))
}

async fn barberos<C: Send + 'static>(
    State(e): State<Estado<C>>,
    Parametros(p): Parametros<ServicioParametro>,
) -> Result<Json<Vec<OpcionBarberoDto>>, ErrorHttp> {
    Ok(Json(
        e.casos
            .publico
            .barberos(IdServicio(p.servicio_id))
            .await
            .map_err(error_http)?,
    ))
}

async fn calendario<C: Send + 'static>(
    State(e): State<Estado<C>>,
    Parametros(p): Parametros<CalendarioParametros>,
) -> Result<Json<CalendarioDto>, ErrorHttp> {
    let dto = e
        .casos
        .publico
        .calendario(IdServicio(p.servicio_id), p.barbero_id.map(IdBarbero), &p.mes)
        .await
        .map_err(error_http)?;
    Ok(Json(dto))
}

async fn horas<C: Send + 'static>(
    State(e): State<Estado<C>>,
    Parametros(p): Parametros<HorasParametros>,
) -> Result<Json<HorasDelDiaDto>, ErrorHttp> {
    let dto = e
        .casos
        .publico
        .horas(
            IdServicio(p.servicio_id),
            p.barbero_id.map(IdBarbero),
            p.fecha,
            Modo::Publico,
        )
        .await
        .map_err(error_http)?;
    Ok(Json(dto))
}

/// `201` con la cita, o `409 conflicto_horario` con `alternativas`.
async fn reservar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    IpCliente(ip): IpCliente,
    encabezados: HeaderMap,
    Entrada(s): Entrada<ReservaSolicitud>,
) -> Result<Response, ErrorHttp> {
    let clave = ip.map_or_else(|| "desconocida".to_owned(), |ip| ip.to_string());
    e.reservas_por_ip
        .intentar(&clave)
        .map_err(ErrorHttp::demasiados_intentos)?;
    let agente = encabezados
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let creada = e
        .casos
        .reserva
        .crear(SolicitudReserva {
            comprobante: s.comprobante,
            servicio_id: IdServicio(s.servicio_id),
            barbero_id: s.barbero_id.map(IdBarbero),
            inicio: s.inicio,
            nombre: s.cliente.nombre,
            correo: s.cliente.correo,
            fecha_nacimiento: s.cliente.fecha_nacimiento,
            autoriza_tratamiento_datos: s.autoriza_tratamiento_datos,
            politica_version_id: s.politica_version_id,
            ip,
            agente,
        })
        .await
        .map_err(error_http)?;
    let ubicacion = creada
        .enlace_gestion
        .rsplit('/')
        .next()
        .map(|token| format!("/api/v1/publico/citas/gestion/{token}"));
    let mut respuesta = (StatusCode::CREATED, Json(creada)).into_response();
    if let Some(valor) = ubicacion.and_then(|u| HeaderValue::from_str(&u).ok()) {
        respuesta.headers_mut().insert(header::LOCATION, valor);
    }
    Ok(respuesta)
}

fn token(ruta: Result<Path<String>, PathRejection>) -> Result<String, ErrorHttp> {
    ruta.map(|Path(t)| t)
        .map_err(|_| ErrorHttp::solicitud_invalida("Enlace inválido."))
}

/// Respuestas privadas: el enlace es personal y no se debe guardar en cachés compartidas.
fn privado(dto: GestionCitaDto) -> Response {
    let mut respuesta = Json(dto).into_response();
    respuesta.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    respuesta
}

async fn ver<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ruta: Result<Path<String>, PathRejection>,
) -> Result<Response, ErrorHttp> {
    Ok(privado(
        e.casos.gestion.ver(&token(ruta)?).await.map_err(error_http)?,
    ))
}

async fn cancelar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ruta: Result<Path<String>, PathRejection>,
) -> Result<Response, ErrorHttp> {
    Ok(privado(
        e.casos
            .gestion
            .cancelar(&token(ruta)?)
            .await
            .map_err(error_http)?,
    ))
}

async fn reprogramar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ruta: Result<Path<String>, PathRejection>,
    Entrada(s): Entrada<ReprogramacionSolicitud>,
) -> Result<Response, ErrorHttp> {
    let dto = e
        .casos
        .gestion
        .reprogramar(&token(ruta)?, s.inicio, s.barbero_id.map(IdBarbero))
        .await
        .map_err(error_http)?;
    Ok(privado(dto))
}

async fn recordatorios<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ruta: Result<Path<String>, PathRejection>,
    Entrada(s): Entrada<RecordatoriosSolicitud>,
) -> Result<Response, ErrorHttp> {
    Ok(privado(
        e.casos
            .gestion
            .recordatorios(&token(ruta)?, s.activos)
            .await
            .map_err(error_http)?,
    ))
}
