//! Rutas de la agenda del personal (con sesión).

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime, Utc};
use clientes::api::NuevoCliente;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio};
use nucleo::{Actor, ZONA_HORARIA};
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};
use serde::Deserialize;

use super::publico::{HorasParametros, ReprogramacionSolicitud};
use super::{Estado, error_http, numero_opcional};
use crate::aplicacion::dto::{CitaDto, DetalleCitaDto, HorasDelDiaDto, TableroDto};
use crate::aplicacion::errores::ErrorAgenda;
use crate::aplicacion::panel::{Propuesta, SolicitudCitaPanel};
use crate::dominio::configuracion::ConfiguracionAgenda;
use crate::dominio::disponibilidad::{Franja, Modo};

pub(crate) fn rutas<C: Send + 'static>() -> Router<Estado<C>> {
    Router::new()
        .route("/agenda/tablero", get(tablero::<C>))
        .route("/agenda/citas", get(listar::<C>).post(crear::<C>))
        .route("/agenda/citas/{id}", get(detalle::<C>))
        .route("/agenda/citas/{id}/reprogramar", post(reprogramar::<C>))
        .route("/agenda/citas/{id}/cancelar", post(cancelar::<C>))
        .route("/agenda/citas/{id}/confirmar", post(confirmar::<C>))
        .route("/agenda/citas/{id}/iniciar", post(iniciar::<C>))
        .route("/agenda/citas/{id}/inasistencia", post(inasistencia::<C>))
        .route("/agenda/disponibilidad/horas", get(disponibilidad::<C>))
        .route("/agenda/conflictos", post(conflictos::<C>))
        .route("/agenda/pendientes-de-cierre", get(pendientes::<C>))
        .route(
            "/agenda/configuracion",
            get(configuracion::<C>).put(guardar_configuracion::<C>),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FechaParametro {
    fecha: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListadoParametros {
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    #[serde(default, deserialize_with = "numero_opcional")]
    barbero_id: Option<i64>,
    estado: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClienteNuevoSolicitud {
    nombre: String,
    celular: String,
    correo: Option<String>,
    fecha_nacimiento: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CitaPanelSolicitud {
    cliente_id: Option<i64>,
    cliente_nuevo: Option<ClienteNuevoSolicitud>,
    servicio_id: i64,
    barbero_id: Option<i64>,
    inicio: DateTime<FixedOffset>,
    nota: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelacionSolicitud {
    motivo: String,
    #[serde(default)]
    avisar_cliente: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RangoFechasSolicitud {
    desde: NaiveDate,
    hasta: NaiveDate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BloqueoPropuesto {
    fecha: NaiveDate,
    desde: String,
    hasta: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FranjaPropuesta {
    dia_semana: u8,
    inicio: String,
    fin: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConflictosSolicitud {
    barbero_id: i64,
    ausencia: Option<RangoFechasSolicitud>,
    bloqueo: Option<BloqueoPropuesto>,
    horario_propuesto: Option<Vec<FranjaPropuesta>>,
}

fn hora(texto: &str) -> Result<NaiveTime, ErrorHttp> {
    NaiveTime::parse_from_str(texto, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(texto, "%H:%M"))
        .map_err(|_| ErrorHttp::solicitud_invalida(format!("Hora inválida «{texto}» (use HH:MM).")))
}

fn id(ruta: Result<Path<i64>, PathRejection>) -> Result<IdCita, ErrorHttp> {
    ruta.map(|Path(id)| IdCita(id))
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

async fn tablero<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<FechaParametro>,
) -> Result<Json<TableroDto>, ErrorHttp> {
    let fecha = p
        .fecha
        .unwrap_or_else(|| Utc::now().with_timezone(&ZONA_HORARIA).date_naive());
    Ok(Json(
        e.casos.panel.tablero(&actor, fecha).await.map_err(error_http)?,
    ))
}

/// Sin fechas: hoy y los próximos 7 días («Hoy», «Mañana» y «Próximos 7 días» del barbero).
async fn listar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ListadoParametros>,
) -> Result<Json<Vec<CitaDto>>, ErrorHttp> {
    let hoy = Utc::now().with_timezone(&ZONA_HORARIA).date_naive();
    let desde = p.desde.unwrap_or(hoy);
    let hasta = p.hasta.unwrap_or_else(|| desde + chrono::Days::new(7));
    let citas = e
        .casos
        .panel
        .listar(&actor, desde, hasta, p.barbero_id.map(IdBarbero), p.estado)
        .await
        .map_err(error_http)?;
    Ok(Json(citas))
}

async fn crear<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<CitaPanelSolicitud>,
) -> Result<Response, ErrorHttp> {
    let solicitud = SolicitudCitaPanel {
        cliente_id: s.cliente_id.map(IdCliente),
        cliente_nuevo: s.cliente_nuevo.map(|c| NuevoCliente {
            nombre: c.nombre,
            celular: c.celular,
            correo: c.correo,
            fecha_nacimiento: c.fecha_nacimiento,
        }),
        servicio_id: IdServicio(s.servicio_id),
        barbero_id: s.barbero_id.map(IdBarbero),
        inicio: s.inicio,
        nota: s.nota,
    };
    let creada = e.casos.panel.crear(&actor, solicitud).await.map_err(error_http)?;
    Ok((StatusCode::CREATED, Json(creada)).into_response())
}

async fn detalle<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .detalle(&actor, id(ruta)?)
            .await
            .map_err(error_http)?,
    ))
}

async fn reprogramar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<ReprogramacionSolicitud>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    let dto = e
        .casos
        .panel
        .reprogramar(&actor, id(ruta)?, s.inicio, s.barbero_id.map(IdBarbero))
        .await
        .map_err(error_http)?;
    Ok(Json(dto))
}

async fn cancelar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<CancelacionSolicitud>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    let dto = e
        .casos
        .panel
        .cancelar(&actor, id(ruta)?, &s.motivo, s.avisar_cliente)
        .await
        .map_err(error_http)?;
    Ok(Json(dto))
}

async fn confirmar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .confirmar(&actor, id(ruta)?)
            .await
            .map_err(error_http)?,
    ))
}

async fn iniciar<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .iniciar(&actor, id(ruta)?)
            .await
            .map_err(error_http)?,
    ))
}

async fn inasistencia<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<DetalleCitaDto>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .inasistencia(&actor, id(ruta)?)
            .await
            .map_err(error_http)?,
    ))
}

/// Variante del personal: permite agendar «ahora mismo» y fuera de la ventana pública.
async fn disponibilidad<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<HorasParametros>,
) -> Result<Json<HorasDelDiaDto>, ErrorHttp> {
    if !matches!(actor, Actor::Administrador { .. }) {
        return Err(error_http(ErrorAgenda::Prohibido));
    }
    let dto = e
        .casos
        .publico
        .horas(
            IdServicio(p.servicio_id),
            p.barbero_id.map(IdBarbero),
            p.fecha,
            Modo::Personal,
        )
        .await
        .map_err(error_http)?;
    Ok(Json(dto))
}

/// Exactamente una propuesta: `ausencia`, `bloqueo` u `horario_propuesto`.
async fn conflictos<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<ConflictosSolicitud>,
) -> Result<Json<Vec<CitaDto>>, ErrorHttp> {
    let propuesta = match (s.ausencia, s.bloqueo, s.horario_propuesto) {
        (Some(a), None, None) => Propuesta::Ausencia {
            desde: a.desde,
            hasta: a.hasta,
        },
        (None, Some(b), None) => {
            let instante = |h: NaiveTime| {
                b.fecha
                    .and_time(h)
                    .and_local_timezone(ZONA_HORARIA)
                    .earliest()
                    .map(|d| d.with_timezone(&Utc))
            };
            match (instante(hora(&b.desde)?), instante(hora(&b.hasta)?)) {
                (Some(inicio), Some(fin)) if fin > inicio => Propuesta::Bloqueo { inicio, fin },
                _ => {
                    return Err(ErrorHttp::solicitud_invalida(
                        "El bloqueo propuesto no es válido.",
                    ));
                }
            }
        }
        (None, None, Some(franjas)) => Propuesta::Horario(
            franjas
                .into_iter()
                .map(|f| {
                    Ok(Franja {
                        dia_semana: f.dia_semana,
                        inicio: hora(&f.inicio)?,
                        fin: hora(&f.fin)?,
                    })
                })
                .collect::<Result<Vec<_>, ErrorHttp>>()?,
        ),
        _ => {
            return Err(ErrorHttp::solicitud_invalida(
                "Envíe una sola propuesta: ausencia, bloqueo u horario_propuesto.",
            ));
        }
    };
    Ok(Json(
        e.casos
            .panel
            .conflictos(&actor, IdBarbero(s.barbero_id), propuesta)
            .await
            .map_err(error_http)?,
    ))
}

async fn pendientes<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<FechaParametro>,
) -> Result<Json<Vec<CitaDto>>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .pendientes_de_cierre(&actor, p.fecha)
            .await
            .map_err(error_http)?,
    ))
}

async fn configuracion<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<ConfiguracionAgenda>, ErrorHttp> {
    Ok(Json(
        e.casos.panel.configuracion(&actor).await.map_err(error_http)?,
    ))
}

async fn guardar_configuracion<C: Send + 'static>(
    State(e): State<Estado<C>>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(c): Entrada<ConfiguracionAgenda>,
) -> Result<Json<ConfiguracionAgenda>, ErrorHttp> {
    Ok(Json(
        e.casos
            .panel
            .guardar_configuracion(&actor, c)
            .await
            .map_err(error_http)?,
    ))
}
