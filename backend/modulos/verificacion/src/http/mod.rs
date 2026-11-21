//! Rutas públicas (relativas a `/api/v1`): `POST /publico/verificaciones` y
//! `POST /publico/verificaciones/{id}/confirmar`. Con límites de intentos (`docs/diseno/api.md`, 1.4).

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use plataforma::http::{Entrada, ErrorHttp, IpCliente};
use plataforma::seguridad::LimitadorIntentos;
use serde::Deserialize;

use crate::api::{CodigoEnviadoDto, VerificacionDto};
use crate::aplicacion::casos::Verificacion;

/// 3 códigos por celular cada 15 minutos y 10 por IP cada hora.
#[derive(Debug)]
pub(crate) struct Limites {
    por_celular: LimitadorIntentos,
    por_ip: LimitadorIntentos,
}

impl Limites {
    pub(crate) fn estandar() -> Self {
        Self {
            por_celular: LimitadorIntentos::nuevo(3, Duration::from_secs(15 * 60)),
            por_ip: LimitadorIntentos::nuevo(10, Duration::from_secs(60 * 60)),
        }
    }
}

pub(crate) struct Estado<C: Send> {
    pub(crate) verificacion: Arc<Verificacion<C>>,
    pub(crate) limites: Arc<Limites>,
}

impl<C: Send> Clone for Estado<C> {
    fn clone(&self) -> Self {
        Self {
            verificacion: self.verificacion.clone(),
            limites: self.limites.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SolicitudCodigo {
    celular: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SolicitudConfirmacion {
    codigo: String,
}

pub(crate) fn rutas<C: Send + 'static>(estado: Estado<C>) -> Router {
    Router::new()
        .route("/publico/verificaciones", post(solicitar::<C>))
        .route("/publico/verificaciones/{id}/confirmar", post(confirmar::<C>))
        .with_state(estado)
}

fn clave_ip(ip: Option<IpAddr>) -> String {
    ip.map_or_else(|| "desconocida".to_owned(), |ip| ip.to_string())
}

async fn solicitar<C: Send + 'static>(
    State(estado): State<Estado<C>>,
    IpCliente(ip): IpCliente,
    Entrada(s): Entrada<SolicitudCodigo>,
) -> Result<(StatusCode, Json<CodigoEnviadoDto>), ErrorHttp> {
    let celular: String = s.celular.chars().filter(char::is_ascii_digit).collect();
    estado
        .limites
        .por_ip
        .intentar(&clave_ip(ip))
        .map_err(ErrorHttp::demasiados_intentos)?;
    estado
        .limites
        .por_celular
        .intentar(&celular)
        .map_err(ErrorHttp::demasiados_intentos)?;
    let enviado = estado.verificacion.solicitar(&s.celular, ip).await?;
    Ok((StatusCode::ACCEPTED, Json(enviado)))
}

async fn confirmar<C: Send + 'static>(
    State(estado): State<Estado<C>>,
    id: Result<Path<i64>, PathRejection>,
    Entrada(s): Entrada<SolicitudConfirmacion>,
) -> Result<Json<VerificacionDto>, ErrorHttp> {
    let Path(id) = id.map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    Ok(Json(estado.verificacion.confirmar(id, &s.codigo).await?))
}
