//! Rutas HTTP del módulo (relativas a `/api/v1`, `docs/diseno/api.md`, sección 2.7).

use std::sync::Arc;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use nucleo::identificadores::IdCliente;
use nucleo::{Pagina, SolicitudPagina};
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::dto::{ClienteFichaDto, ClienteListadoDto, ClienteRapidoDto, SolicitudTitularDto};
use crate::aplicacion::{CasosDeUso, SolicitudCliente};

type Casos<C> = State<Arc<CasosDeUso<C>>>;

pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    Router::new()
        .route("/clientes", get(listar::<C>).post(crear::<C>))
        .route("/clientes/buscar", get(buscar::<C>))
        .route("/clientes/{id}", get(ficha::<C>).put(editar::<C>))
        .route("/clientes/{id}/anonimizacion", post(anonimizar::<C>))
        .route(
            "/clientes/solicitudes-titular",
            get(solicitudes::<C>).post(registrar_solicitud::<C>),
        )
        .route(
            "/clientes/solicitudes-titular/{id}",
            patch(responder_solicitud::<C>),
        )
        .with_state(casos)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListadoParametros {
    q: Option<String>,
    segmento: Option<String>,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusquedaParametros {
    q: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClienteSolicitud {
    nombre: String,
    celular: String,
    correo: Option<String>,
    fecha_nacimiento: Option<NaiveDate>,
    preferencias: Option<String>,
    recordatorios_activos: Option<bool>,
}

impl From<ClienteSolicitud> for SolicitudCliente {
    fn from(s: ClienteSolicitud) -> Self {
        SolicitudCliente {
            nombre: s.nombre,
            celular: s.celular,
            correo: s.correo,
            fecha_nacimiento: s.fecha_nacimiento,
            preferencias: s.preferencias,
            recordatorios_activos: s.recordatorios_activos,
        }
    }
}

fn id(ruta: Result<Path<i64>, PathRejection>) -> Result<IdCliente, ErrorHttp> {
    ruta.map(|Path(id)| IdCliente(id))
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

async fn listar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ListadoParametros>,
) -> Result<Json<Pagina<ClienteListadoDto>>, ErrorHttp> {
    let defecto = SolicitudPagina::default();
    let pagina = SolicitudPagina {
        pagina: p.pagina.unwrap_or(defecto.pagina),
        por_pagina: p.por_pagina.unwrap_or(defecto.por_pagina),
    };
    Ok(Json(
        casos.listar.ejecutar(&actor, p.q, p.segmento, pagina).await?,
    ))
}

async fn buscar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<BusquedaParametros>,
) -> Result<Json<Vec<ClienteRapidoDto>>, ErrorHttp> {
    Ok(Json(casos.buscar.ejecutar(&actor, &p.q).await?))
}

async fn crear<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<ClienteSolicitud>,
) -> Result<Response, ErrorHttp> {
    let ficha = casos.crear.ejecutar(&actor, s.into()).await?;
    let ubicacion = format!("/api/v1/clientes/{}", ficha.resumen.id);
    Ok((StatusCode::CREATED, [(header::LOCATION, ubicacion)], Json(ficha)).into_response())
}

async fn ficha<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<ClienteFichaDto>, ErrorHttp> {
    Ok(Json(casos.ficha.ejecutar(&actor, id(ruta)?).await?))
}

async fn editar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<ClienteSolicitud>,
) -> Result<Json<ClienteFichaDto>, ErrorHttp> {
    Ok(Json(casos.editar.ejecutar(&actor, id(ruta)?, s.into()).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnonimizacionSolicitud {
    motivo: String,
}

/// Supresión a petición del titular (RF-CLI-07). `204` sin cuerpo.
async fn anonimizar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<AnonimizacionSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    let Ok(Path(id)) = ruta else {
        return Err(ErrorHttp::solicitud_invalida(
            "El identificador debe ser un número.",
        ));
    };
    casos.titular.anonimizar(&actor, IdCliente(id), &s.motivo).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltroSolicitudes {
    #[serde(default)]
    abiertas: bool,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

async fn solicitudes<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(f): Parametros<FiltroSolicitudes>,
) -> Result<Json<Pagina<SolicitudTitularDto>>, ErrorHttp> {
    let defecto = SolicitudPagina::default();
    let pagina = SolicitudPagina {
        pagina: f.pagina.unwrap_or(defecto.pagina),
        por_pagina: f.por_pagina.unwrap_or(defecto.por_pagina),
    };
    Ok(Json(casos.titular.solicitudes(&actor, f.abiertas, pagina).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NuevaSolicitudTitular {
    cliente_id: Option<i64>,
    /// `consulta`, `actualizacion` o `supresion`.
    tipo: String,
    descripcion: String,
}

async fn registrar_solicitud<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<NuevaSolicitudTitular>,
) -> Result<Response, ErrorHttp> {
    let dto = casos
        .titular
        .registrar(&actor, s.cliente_id.map(IdCliente), &s.tipo, &s.descripcion)
        .await?;
    Ok((StatusCode::CREATED, Json(dto)).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RespuestaTitular {
    respuesta: String,
}

async fn responder_solicitud<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<RespuestaTitular>,
) -> Result<Json<SolicitudTitularDto>, ErrorHttp> {
    let Ok(Path(id)) = ruta else {
        return Err(ErrorHttp::solicitud_invalida(
            "El identificador debe ser un número.",
        ));
    };
    Ok(Json(casos.titular.responder(&actor, id, &s.respuesta).await?))
}
