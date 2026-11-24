//! Vigilancia de la operación (RNF-OBS-02): métricas de la API (errores del servidor y
//! latencia), cola de mensajes (atrasos y fallos de correo y WhatsApp), webhooks y worker, con
//! alertas de umbrales fijos.
//!
//! - `GET /api/salud/operacion`, sin sesión, para el monitor externo (UptimeRobot o similar):
//!   `200 {"estado":"ok"}` o `503 {"estado":"alerta","alertas":[códigos]}`. Solo códigos: ni
//!   cifras ni datos de nadie.
//! - `GET /api/v1/sistema/estado`, solo la administradora: todo el detalle.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset};
use notificaciones::api::EstadoCola;
use nucleo::ZONA_HORARIA;
use plataforma::http::{ActorAutenticado, ErrorHttp, MetricasHttp, ResumenHttp, ResumenRuta};
use serde::Serialize;
use serde_json::json;
use trabajos::api::EstadoTrabajos;

use crate::composicion::Composicion;

/// Respuestas `5xx` en los últimos 5 minutos que ya son una alerta.
const ERRORES_DEL_SERVIDOR: u64 = 5;
/// p95 de la latencia (sin los PDF) por encima del cual la API está lenta…
const LATENCIA_P95_MS: u64 = 1_000;
/// …si se midieron al menos estas peticiones (con dos o tres no se concluye nada).
const MEDIDAS_MINIMAS: u64 = 20;
/// Minutos de atraso de un correo o un WhatsApp en la cola.
const ATRASO_COLA_MIN: i64 = 15;
/// WhatsApp fallidos en la última hora.
const WHATSAPP_FALLIDOS_HORA: i64 = 3;
/// Minutos sin que el worker tome lo vencido (trabajos o webhooks).
const ATRASO_WORKER_MIN: i64 = 10;
/// Minutos sin que termine ningún trabajo: los recordatorios corren cada 5.
const WORKER_SIN_ACTIVIDAD_MIN: i64 = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlertaDto {
    pub(crate) codigo: &'static str,
    pub(crate) mensaje: String,
}

fn alerta(codigo: &'static str, mensaje: String) -> AlertaDto {
    AlertaDto { codigo, mensaje }
}

/// Alertas (algo falla y hay que actuar) e informativos (un estado conocido que no es una falla,
/// como WhatsApp sin proveedor mientras no estén las plantillas de Meta).
pub(crate) fn evaluar(
    api: &ResumenHttp,
    cola: &EstadoCola,
    trabajos: &EstadoTrabajos,
) -> (Vec<AlertaDto>, Vec<AlertaDto>) {
    let mut alertas = Vec::new();
    let mut informativos = Vec::new();
    if api.errores_del_servidor >= ERRORES_DEL_SERVIDOR {
        alertas.push(alerta(
            "errores_del_servidor",
            format!(
                "{} respuestas con error del servidor en los últimos 5 minutos: revise los logs de la API (ERROR).",
                api.errores_del_servidor
            ),
        ));
    }
    if let Some(l) = api.latencia
        && l.medidas >= MEDIDAS_MINIMAS
        && l.p95_ms > LATENCIA_P95_MS
    {
        alertas.push(alerta(
            "api_lenta",
            format!(
                "La API está lenta: el 5 % de las peticiones de los últimos 5 minutos tardó más de {} ms.",
                LATENCIA_P95_MS
            ),
        ));
    }
    let worker_parado = trabajos
        .minutos_desde_el_ultimo
        .is_some_and(|m| m > WORKER_SIN_ACTIVIDAD_MIN)
        || trabajos.atraso_min.is_some_and(|m| m >= ATRASO_WORKER_MIN)
        || cola.webhooks_atraso_min.is_some_and(|m| m >= ATRASO_WORKER_MIN);
    if worker_parado {
        alertas.push(alerta(
            "worker_detenido",
            "El worker no está trabajando: hay tareas o webhooks vencidos sin atender (recordatorios, envíos). Revise el contenedor del worker.".into(),
        ));
    }
    if trabajos.atascados > 0 {
        alertas.push(alerta(
            "trabajos_atascados",
            format!(
                "{} trabajos llevan más de 15 minutos en proceso sin terminar.",
                trabajos.atascados
            ),
        ));
    }
    let correo = &cola.correo;
    if correo.configurado && correo.atraso_min.is_some_and(|m| m >= ATRASO_COLA_MIN) {
        alertas.push(alerta(
            "correos_atrasados",
            format!(
                "{} correos esperan en la cola desde hace {} minutos: SMTP caído o el worker detenido.",
                correo.pendientes,
                correo.atraso_min.unwrap_or_default()
            ),
        ));
    }
    if !correo.configurado && correo.pendientes > 0 {
        alertas.push(alerta(
            "correo_sin_configurar",
            format!(
                "{} correos esperan en la cola y no hay SMTP configurado (invitaciones y recuperación de contraseña no llegan).",
                correo.pendientes
            ),
        ));
    }
    let whatsapp = &cola.whatsapp;
    if whatsapp.configurado {
        if whatsapp.atraso_min.is_some_and(|m| m >= ATRASO_COLA_MIN) {
            alertas.push(alerta(
                "whatsapp_atrasados",
                format!(
                    "{} WhatsApp esperan en la cola desde hace {} minutos.",
                    whatsapp.pendientes,
                    whatsapp.atraso_min.unwrap_or_default()
                ),
            ));
        }
        if whatsapp.fallidos_ultima_hora >= WHATSAPP_FALLIDOS_HORA {
            alertas.push(alerta(
                "whatsapp_fallidos",
                format!(
                    "{} WhatsApp fallaron en la última hora: revise las plantillas y la cuenta de Meta.",
                    whatsapp.fallidos_ultima_hora
                ),
            ));
        }
    } else if whatsapp.pendientes > 0 {
        informativos.push(alerta(
            "whatsapp_sin_proveedor",
            format!(
                "{} WhatsApp esperan en la cola: aún no hay proveedor (Meta) configurado.",
                whatsapp.pendientes
            ),
        ));
    }
    if let Some(celulares) = cola.whatsapp_solo_a {
        informativos.push(alerta(
            "whatsapp_solo_a",
            format!(
                "WhatsApp sale por Meta solo a {celulares} celular(es) de DIAMANTE_WHATSAPP_SOLO_A; al resto se simula. En producción, quite la variable."
            ),
        ));
    }
    (alertas, informativos)
}

#[derive(Debug, Serialize)]
struct ApiDto {
    segundos_en_marcha: u64,
    ultimos_5_min: ResumenHttp,
    ultimos_15_min: ResumenHttp,
    desde_el_arranque: ResumenHttp,
    /// Las que tuvieron errores del servidor y las más lentas, desde el arranque.
    rutas_destacadas: Vec<ResumenRuta>,
}

#[derive(Debug, Serialize)]
struct EstadoSistemaDto {
    generado_en: DateTime<FixedOffset>,
    /// `ok` o `alerta`.
    estado: &'static str,
    alertas: Vec<AlertaDto>,
    informativos: Vec<AlertaDto>,
    /// Las métricas son de este proceso de la API (cada réplica lleva las suyas).
    api: ApiDto,
    cola: EstadoCola,
    trabajos: EstadoTrabajos,
}

#[derive(Clone)]
struct Estado {
    composicion: Composicion,
    metricas: Arc<MetricasHttp>,
}

async fn tomar_foto(e: &Estado) -> Result<EstadoSistemaDto, String> {
    let cola = e
        .composicion
        .notificaciones
        .estado_cola()
        .await
        .map_err(|error| error.to_string())?;
    let trabajos = e
        .composicion
        .trabajos
        .estado()
        .await
        .map_err(|error| error.to_string())?;
    let ultimos_5_min = e.metricas.ultimos(5);
    let (alertas, informativos) = evaluar(&ultimos_5_min, &cola, &trabajos);
    Ok(EstadoSistemaDto {
        generado_en: e
            .composicion
            .reloj
            .ahora()
            .with_timezone(&ZONA_HORARIA)
            .fixed_offset(),
        estado: if alertas.is_empty() { "ok" } else { "alerta" },
        alertas,
        informativos,
        api: ApiDto {
            segundos_en_marcha: e.metricas.segundos_en_marcha(),
            ultimos_5_min,
            ultimos_15_min: e.metricas.ultimos(15),
            desde_el_arranque: e.metricas.desde_el_arranque(),
            rutas_destacadas: e.metricas.rutas_destacadas(10),
        },
        cola,
        trabajos,
    })
}

async fn estado(
    State(e): State<Estado>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<EstadoSistemaDto>, ErrorHttp> {
    if !actor.es_administrador() {
        return Err(ErrorHttp::nuevo(
            StatusCode::FORBIDDEN,
            "prohibido",
            "No tienes permiso para hacer esto.",
        ));
    }
    tomar_foto(&e).await.map(Json).map_err(|error| {
        tracing::error!(error, "no se pudo tomar el estado del sistema");
        ErrorHttp::interno()
    })
}

async fn operacion(State(e): State<Estado>) -> Response {
    match tomar_foto(&e).await {
        Ok(foto) if foto.alertas.is_empty() => Json(json!({ "estado": "ok" })).into_response(),
        Ok(foto) => {
            let codigos: Vec<&str> = foto.alertas.iter().map(|a| a.codigo).collect();
            tracing::warn!(alertas = ?codigos, "la vigilancia encontró alertas");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "estado": "alerta", "alertas": codigos })),
            )
                .into_response()
        }
        Err(error) => {
            tracing::error!(error, "la vigilancia no pudo consultar la base");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "estado": "sin_base" })),
            )
                .into_response()
        }
    }
}

/// `GET /sistema/estado` (bajo `/api/v1`).
pub(crate) fn rutas_panel(composicion: &Composicion) -> Router {
    Router::new()
        .route("/sistema/estado", get(estado))
        .with_state(Estado {
            composicion: composicion.clone(),
            metricas: composicion.metricas.clone(),
        })
}

/// `GET /api/salud/operacion`.
pub(crate) fn ruta_salud(composicion: &Composicion) -> Router {
    Router::new()
        .route("/api/salud/operacion", get(operacion))
        .with_state(Estado {
            composicion: composicion.clone(),
            metricas: composicion.metricas.clone(),
        })
}

#[cfg(test)]
mod pruebas {
    use notificaciones::api::EstadoCanal;
    use plataforma::http::Latencia;

    use super::*;

    fn codigos(r: &(Vec<AlertaDto>, Vec<AlertaDto>)) -> (Vec<&'static str>, Vec<&'static str>) {
        (
            r.0.iter().map(|a| a.codigo).collect(),
            r.1.iter().map(|a| a.codigo).collect(),
        )
    }

    fn sano() -> (ResumenHttp, EstadoCola, EstadoTrabajos) {
        let api = ResumenHttp {
            peticiones: 200,
            exitosas: 190,
            errores_del_cliente: 6,
            errores_del_servidor: 4,
            latencia: Some(Latencia {
                medidas: 200,
                p50_ms: 25,
                p95_ms: 250,
                p99_ms: 1_000,
            }),
            ..Default::default()
        };
        let cola = EstadoCola {
            correo: EstadoCanal {
                configurado: true,
                pendientes: 2,
                atraso_min: Some(1),
                ..Default::default()
            },
            whatsapp: EstadoCanal {
                configurado: true,
                fallidos_ultima_hora: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        let trabajos = EstadoTrabajos {
            minutos_desde_el_ultimo: Some(4),
            ..Default::default()
        };
        (api, cola, trabajos)
    }

    #[test]
    fn todo_en_orden_por_debajo_de_los_umbrales() {
        let (api, cola, trabajos) = sano();
        assert_eq!(codigos(&evaluar(&api, &cola, &trabajos)), (vec![], vec![]));
    }

    #[test]
    fn cada_umbral_dispara_su_alerta() {
        let (mut api, mut cola, mut trabajos) = sano();
        api.errores_del_servidor = 5;
        api.latencia = Some(Latencia {
            medidas: 20,
            p50_ms: 100,
            p95_ms: 2_500,
            p99_ms: 5_000,
        });
        cola.correo.atraso_min = Some(15);
        cola.whatsapp.atraso_min = Some(20);
        cola.whatsapp.fallidos_ultima_hora = 3;
        trabajos.atascados = 1;
        trabajos.minutos_desde_el_ultimo = Some(16);
        assert_eq!(
            codigos(&evaluar(&api, &cola, &trabajos)).0,
            [
                "errores_del_servidor",
                "api_lenta",
                "worker_detenido",
                "trabajos_atascados",
                "correos_atrasados",
                "whatsapp_atrasados",
                "whatsapp_fallidos"
            ]
        );
    }

    #[test]
    fn pocas_medidas_no_son_una_api_lenta() {
        let (mut api, cola, trabajos) = sano();
        api.latencia = Some(Latencia {
            medidas: 19,
            p50_ms: 5_000,
            p95_ms: 10_000,
            p99_ms: 10_000,
        });
        assert!(evaluar(&api, &cola, &trabajos).0.is_empty());
    }

    #[test]
    fn el_worker_se_da_por_detenido_con_cualquier_senal() {
        let (api, cola, _) = sano();
        for trabajos in [
            EstadoTrabajos {
                atraso_min: Some(10),
                ..Default::default()
            },
            EstadoTrabajos {
                minutos_desde_el_ultimo: Some(16),
                ..Default::default()
            },
        ] {
            assert_eq!(codigos(&evaluar(&api, &cola, &trabajos)).0, ["worker_detenido"]);
        }
        let mut cola_con_webhooks = cola;
        cola_con_webhooks.webhooks_atraso_min = Some(12);
        assert_eq!(
            codigos(&evaluar(&api, &cola_con_webhooks, &EstadoTrabajos::default())).0,
            ["worker_detenido"]
        );
        // Recién instalado: nunca ha terminado un trabajo y no hay nada vencido.
        assert!(evaluar(&api, &cola, &EstadoTrabajos::default()).0.is_empty());
    }

    #[test]
    fn sin_proveedor_de_whatsapp_es_informativo_y_sin_smtp_es_alerta() {
        let (api, mut cola, trabajos) = sano();
        cola.whatsapp = EstadoCanal {
            configurado: false,
            pendientes: 12,
            atraso_min: Some(600),
            fallidos_ultima_hora: 0,
            ..Default::default()
        };
        assert_eq!(
            codigos(&evaluar(&api, &cola, &trabajos)),
            (vec![], vec!["whatsapp_sin_proveedor"])
        );
        cola.correo.configurado = false;
        assert_eq!(
            codigos(&evaluar(&api, &cola, &trabajos)).0,
            ["correo_sin_configurar"]
        );
    }

    #[test]
    fn whatsapp_solo_a_unos_celulares_es_informativo() {
        let (api, mut cola, trabajos) = sano();
        cola.whatsapp_solo_a = Some(2);
        assert_eq!(
            codigos(&evaluar(&api, &cola, &trabajos)),
            (vec![], vec!["whatsapp_solo_a"])
        );
    }
}
