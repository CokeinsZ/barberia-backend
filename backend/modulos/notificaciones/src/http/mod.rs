//! Rutas del módulo (bajo `/api/v1`):
//!
//! - Panel: avisos internos y registro de mensajes (solo administradores).
//! - `/webhooks/whatsapp`: verificación de Meta y eventos firmados (`X-Hub-Signature-256`).
//! - `/_pruebas/mensajes` (solo con `DIAMANTE_PRUEBAS_E2E=true`): mensajes del WhatsApp simulado.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nucleo::{Actor, Pagina, Reloj, SolicitudPagina};
use plataforma::configuracion::WebhookWhatsApp;
use plataforma::http::{ActorAutenticado, ErrorHttp, Parametros};
use plataforma::seguridad::{iguales_en_tiempo_constante, verificar_firma_webhook};
use serde::Deserialize;
use sqlx::PgPool;

use crate::infraestructura::postgres::{
    AvisoDto, FiltroMensajes, MensajeDePrueba, MensajeDto, guardar_webhook, listar_avisos, listar_mensajes,
    marcar_aviso_leido, mensajes_recientes,
};

#[derive(Clone)]
pub(crate) struct Estado {
    pub(crate) pool: PgPool,
    pub(crate) reloj: Arc<dyn Reloj>,
    pub(crate) webhook: Option<Arc<WebhookWhatsApp>>,
}

pub(crate) fn rutas(estado: Estado) -> Router {
    Router::new()
        .route("/avisos", get(avisos))
        .route("/avisos/{id}/leido", post(leido))
        .route("/mensajes", get(mensajes))
        .route("/webhooks/whatsapp", get(verificar_webhook).post(recibir_webhook))
        .with_state(estado)
}

pub(crate) fn rutas_de_pruebas(pool: PgPool) -> Router {
    Router::new()
        .route("/_pruebas/mensajes", get(mensajes_de_prueba))
        .with_state(pool)
}

fn solo_administrador(actor: &Actor) -> Result<i64, ErrorHttp> {
    match actor {
        Actor::Administrador { usuario } => Ok(usuario.0),
        _ => Err(ErrorHttp::nuevo(
            StatusCode::FORBIDDEN,
            "prohibido",
            "No tienes permiso para esta acción.",
        )),
    }
}

fn interno(error: nucleo::ErrorPersistencia) -> ErrorHttp {
    tracing::error!(error = %error, "error de persistencia en notificaciones");
    ErrorHttp::interno()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltroAvisos {
    #[serde(default)]
    no_leidos: bool,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

fn pagina(pagina: Option<u32>, por_pagina: Option<u32>) -> SolicitudPagina {
    let defecto = SolicitudPagina::default();
    SolicitudPagina {
        pagina: pagina.unwrap_or(defecto.pagina),
        por_pagina: por_pagina.unwrap_or(defecto.por_pagina),
    }
}

/// `GET /avisos?no_leidos=true` (RF-DAS-08, RF-NOT-07, RF-BAR-14).
async fn avisos(
    State(e): State<Estado>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(f): Parametros<FiltroAvisos>,
) -> Result<Json<Pagina<AvisoDto>>, ErrorHttp> {
    solo_administrador(&actor)?;
    let avisos = listar_avisos(&e.pool, f.no_leidos, pagina(f.pagina, f.por_pagina))
        .await
        .map_err(interno)?;
    Ok(Json(avisos))
}

async fn leido(
    State(e): State<Estado>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Result<Path<i64>, PathRejection>,
) -> Result<Json<AvisoDto>, ErrorHttp> {
    let usuario = solo_administrador(&actor)?;
    let Ok(Path(id)) = ruta else {
        return Err(ErrorHttp::solicitud_invalida(
            "El identificador debe ser un número.",
        ));
    };
    marcar_aviso_leido(&e.pool, id, usuario, e.reloj.ahora())
        .await
        .map_err(interno)?
        .map(Json)
        .ok_or_else(|| ErrorHttp::nuevo(StatusCode::NOT_FOUND, "no_encontrado", "El aviso no existe."))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltroRegistro {
    destinatario: Option<String>,
    estado: Option<String>,
    canal: Option<String>,
    plantilla: Option<String>,
    pagina: Option<u32>,
    por_pagina: Option<u32>,
}

/// `GET /mensajes?destinatario=&estado=&canal=&plantilla=` (RF-NOT-07): registro sin variables.
async fn mensajes(
    State(e): State<Estado>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(f): Parametros<FiltroRegistro>,
) -> Result<Json<Pagina<MensajeDto>>, ErrorHttp> {
    solo_administrador(&actor)?;
    let sin_espacios = |valor: Option<String>| {
        valor
            .map(|v| v.chars().filter(|c| !c.is_whitespace()).collect::<String>())
            .filter(|v| !v.is_empty())
    };
    let filtro = FiltroMensajes {
        destinatario: sin_espacios(f.destinatario),
        estado: sin_espacios(f.estado),
        canal: sin_espacios(f.canal),
        plantilla: sin_espacios(f.plantilla),
    };
    let pagina = listar_mensajes(&e.pool, &filtro, pagina(f.pagina, f.por_pagina))
        .await
        .map_err(interno)?;
    Ok(Json(pagina))
}

#[derive(Debug, Deserialize)]
struct Verificacion {
    #[serde(rename = "hub.mode")]
    modo: Option<String>,
    #[serde(rename = "hub.verify_token")]
    token: Option<String>,
    #[serde(rename = "hub.challenge")]
    reto: Option<String>,
}

/// `GET /webhooks/whatsapp`: Meta registra el webhook enviando el token de verificación y
/// espera que le devolvamos el reto.
async fn verificar_webhook(State(e): State<Estado>, Parametros(v): Parametros<Verificacion>) -> Response {
    let Some(config) = &e.webhook else {
        return ErrorHttp::ruta_no_encontrada().into_response();
    };
    let token_valido = v.token.as_deref().is_some_and(|token| {
        iguales_en_tiempo_constante(token.as_bytes(), config.token_verificacion.exponer().as_bytes())
    });
    match (v.modo.as_deref(), v.reto) {
        (Some("subscribe"), Some(reto)) if token_valido => (StatusCode::OK, reto).into_response(),
        _ => ErrorHttp::nuevo(
            StatusCode::FORBIDDEN,
            "prohibido",
            "Token de verificación inválido.",
        )
        .into_response(),
    }
}

/// `POST /webhooks/whatsapp`: verifica la firma con el cuerpo exacto, guarda el evento y
/// responde `200` de inmediato; el worker lo procesa después (RNF-SEG-10).
async fn recibir_webhook(
    State(e): State<Estado>,
    encabezados: HeaderMap,
    cuerpo: Bytes,
) -> Result<StatusCode, ErrorHttp> {
    let Some(config) = &e.webhook else {
        return Err(ErrorHttp::ruta_no_encontrada());
    };
    let firma = encabezados
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !verificar_firma_webhook(config.secreto_app.exponer().as_bytes(), &cuerpo, firma) {
        tracing::warn!("webhook de WhatsApp con firma inválida");
        return Err(ErrorHttp::nuevo(
            StatusCode::UNAUTHORIZED,
            "firma_invalida",
            "La firma del webhook no es válida.",
        ));
    }
    let carga: serde_json::Value = serde_json::from_slice(&cuerpo)
        .map_err(|_| ErrorHttp::solicitud_invalida("El cuerpo del webhook no es JSON."))?;
    guardar_webhook(&e.pool, &carga, e.reloj.ahora())
        .await
        .map_err(interno)?;
    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiltroPrueba {
    destinatario: String,
}

async fn mensajes_de_prueba(
    State(pool): State<PgPool>,
    Parametros(f): Parametros<FiltroPrueba>,
) -> Result<Json<Vec<MensajeDePrueba>>, ErrorHttp> {
    let destinatario: String = f.destinatario.chars().filter(|c| !c.is_whitespace()).collect();
    mensajes_recientes(&pool, &destinatario)
        .await
        .map(Json)
        .map_err(interno)
}
