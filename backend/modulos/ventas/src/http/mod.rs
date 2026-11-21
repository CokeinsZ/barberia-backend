//! Rutas del módulo (bajo `/api/v1`): ventas, caja del día, cierres y métodos de pago.

use std::sync::Arc;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use clientes::api::NuevoCliente;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio, IdVenta};
use nucleo::{Pagina, SolicitudPagina};
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::CasosDeUso;
use crate::aplicacion::caja::SolicitudCierre;
use crate::aplicacion::dto::{CajaDto, CatalogosDto, CierreDto, MetodoPagoDto, ResumenBarberoDto, VentaDto};
use crate::aplicacion::puertos::FiltroVentas;
use crate::aplicacion::ventas::{ClienteVenta, SolicitudVenta};

type Casos<C> = State<Arc<CasosDeUso<C>>>;

pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    Router::new()
        .route("/ventas", get(listar::<C>).post(registrar::<C>))
        .route("/ventas/catalogos", get(catalogos::<C>))
        .route("/ventas/caja", get(caja::<C>))
        .route("/ventas/resumen-barbero", get(resumen_barbero::<C>))
        .route("/ventas/cierres", get(cierre::<C>).post(cerrar::<C>))
        .route("/ventas/metodos-pago", get(metodos::<C>))
        .route("/ventas/metodos-pago/{id}", patch(cambiar_metodo::<C>))
        .route("/ventas/{id}", get(detalle::<C>))
        .route("/ventas/{id}/anulacion", post(anular::<C>))
        .with_state(casos)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum ClienteSolicitud {
    Registrado {
        id: i64,
    },
    Nuevo {
        nombre: String,
        celular: String,
        correo: Option<String>,
        fecha_nacimiento: Option<NaiveDate>,
    },
    Ocasional,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescuentoSolicitud {
    descuento_id: i16,
    motivo_descuento_id: i16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PagoSolicitud {
    metodo_pago_id: i16,
    monto: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VentaSolicitud {
    cita_id: Option<i64>,
    cliente: Option<ClienteSolicitud>,
    barbero_id: Option<i64>,
    servicio_ids: Vec<i64>,
    descuento: Option<DescuentoSolicitud>,
    #[serde(default)]
    propina: i64,
    #[serde(default)]
    pagos: Vec<PagoSolicitud>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltroListado {
    fecha: Option<NaiveDate>,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    barbero_id: Option<i64>,
    cliente_id: Option<i64>,
    #[serde(default)]
    incluir_anuladas: bool,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FechaYBarbero {
    fecha: Option<NaiveDate>,
    barbero_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SoloFecha {
    fecha: NaiveDate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnulacionSolicitud {
    motivo: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContadoSolicitud {
    metodo_pago_id: i16,
    monto: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CierreSolicitud {
    fecha: NaiveDate,
    #[serde(default)]
    contado: Vec<ContadoSolicitud>,
    observacion: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetodoSolicitud {
    activo: bool,
}

fn id_venta(ruta: Result<Path<i64>, PathRejection>) -> Result<IdVenta, ErrorHttp> {
    ruta.map(|Path(id)| IdVenta(id))
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

async fn catalogos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<CatalogosDto>, ErrorHttp> {
    Ok(Json(casos.ventas.catalogos(&actor).await?))
}

/// `201` con la venta (ver el ejemplo de `docs/diseno/api.md`, 3.2).
async fn registrar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<VentaSolicitud>,
) -> Result<Response, ErrorHttp> {
    let solicitud = SolicitudVenta {
        cita: s.cita_id.map(IdCita),
        cliente: s.cliente.map(|c| match c {
            ClienteSolicitud::Registrado { id } => ClienteVenta::Registrado(IdCliente(id)),
            ClienteSolicitud::Nuevo {
                nombre,
                celular,
                correo,
                fecha_nacimiento,
            } => ClienteVenta::Nuevo(NuevoCliente {
                nombre,
                celular,
                correo,
                fecha_nacimiento,
            }),
            ClienteSolicitud::Ocasional => ClienteVenta::Ocasional,
        }),
        barbero: s.barbero_id.map(IdBarbero),
        servicios: s.servicio_ids.into_iter().map(IdServicio).collect(),
        descuento: s.descuento.map(|d| (d.descuento_id, d.motivo_descuento_id)),
        propina: s.propina,
        pagos: s.pagos.into_iter().map(|p| (p.metodo_pago_id, p.monto)).collect(),
    };
    let venta = casos.ventas.registrar(&actor, solicitud).await?;
    Ok((StatusCode::CREATED, Json(venta)).into_response())
}

/// Sin fechas ni cliente: las de hoy. Con `cliente_id`: su historial completo.
async fn listar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(f): Parametros<FiltroListado>,
) -> Result<Json<Pagina<VentaDto>>, ErrorHttp> {
    let (desde, hasta) = match (f.fecha, f.desde, f.hasta, f.cliente_id) {
        (Some(fecha), _, _, _) => (Some(fecha), Some(fecha)),
        (None, None, None, None) => {
            let hoy = casos.caja.hoy();
            (Some(hoy), Some(hoy))
        }
        (None, desde, hasta, _) => (desde, hasta),
    };
    let defecto = SolicitudPagina::default();
    let pagina = SolicitudPagina {
        pagina: f.pagina.unwrap_or(defecto.pagina),
        por_pagina: f.por_pagina.unwrap_or(defecto.por_pagina),
    };
    let filtro = FiltroVentas {
        desde,
        hasta,
        barbero: f.barbero_id.map(IdBarbero),
        cliente: f.cliente_id.map(IdCliente),
        solo_registradas: !f.incluir_anuladas,
    };
    Ok(Json(casos.ventas.listar(&actor, filtro, pagina).await?))
}

async fn detalle<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<VentaDto>, ErrorHttp> {
    Ok(Json(casos.ventas.detalle(&actor, id_venta(ruta)?).await?))
}

async fn anular<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<AnulacionSolicitud>,
) -> Result<Json<VentaDto>, ErrorHttp> {
    Ok(Json(
        casos.ventas.anular(&actor, id_venta(ruta)?, &s.motivo).await?,
    ))
}

async fn caja<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<FechaYBarbero>,
) -> Result<Json<CajaDto>, ErrorHttp> {
    let fecha = p.fecha.unwrap_or_else(|| casos.caja.hoy());
    Ok(Json(
        casos
            .caja
            .caja(&actor, fecha, p.barbero_id.map(IdBarbero))
            .await?,
    ))
}

async fn resumen_barbero<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<FechaYBarbero>,
) -> Result<Json<ResumenBarberoDto>, ErrorHttp> {
    let fecha = p.fecha.unwrap_or_else(|| casos.caja.hoy());
    Ok(Json(
        casos
            .caja
            .resumen_barbero(&actor, fecha, p.barbero_id.map(IdBarbero))
            .await?,
    ))
}

async fn cierre<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<SoloFecha>,
) -> Result<Json<CierreDto>, ErrorHttp> {
    Ok(Json(casos.caja.cierre(&actor, p.fecha).await?))
}

async fn cerrar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(s): Entrada<CierreSolicitud>,
) -> Result<Response, ErrorHttp> {
    let solicitud = SolicitudCierre {
        fecha: s.fecha,
        contado: s
            .contado
            .into_iter()
            .map(|c| (c.metodo_pago_id, c.monto))
            .collect(),
        observacion: s.observacion,
    };
    let cierre = casos.caja.cerrar(&actor, solicitud).await?;
    Ok((StatusCode::CREATED, Json(cierre)).into_response())
}

async fn metodos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<Vec<MetodoPagoDto>>, ErrorHttp> {
    Ok(Json(casos.caja.metodos(&actor).await?))
}

async fn cambiar_metodo<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i16>, PathRejection>,
    Entrada(s): Entrada<MetodoSolicitud>,
) -> Result<Json<MetodoPagoDto>, ErrorHttp> {
    let Ok(Path(id)) = ruta else {
        return Err(ErrorHttp::solicitud_invalida(
            "El identificador debe ser un número.",
        ));
    };
    Ok(Json(casos.caja.cambiar_metodo(&actor, id, s.activo).await?))
}
