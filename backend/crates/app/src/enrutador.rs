//! Enrutador HTTP: monta `/api/v1` una sola vez con las rutas de cada módulo y agrega las
//! capas transversales: id de solicitud, trazas (con la plantilla de la ruta, nunca la URL con
//! sus tokens o datos personales), cabeceras de seguridad, IP del cliente, límites de uso por
//! IP y por sesión, sesión, tiempo máximo y verificación de `Origin`.
//!
//! Tiempo máximo: 30 s por petición, salvo los reportes (120 s), cuyos PDF pueden tardar
//! (RNF-REN-04: hasta 30 s el de todos los barberos, en el servidor de producción).

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::get;
use axum::{Extension, Router};
use plataforma::http::{
    anotar_ruta, cabeceras_de_seguridad, hsts, limitar_uso, ruta_no_encontrada, verificar_origen,
};
use tower_http::LatencyUnit;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::Level;

use crate::composicion::Composicion;
use crate::salud;

/// Tiempo máximo de una petición común.
const TIEMPO_MAXIMO: Duration = Duration::from_secs(30);
/// Tiempo máximo de los reportes (los PDF esperan a Gotenberg).
const TIEMPO_MAXIMO_REPORTES: Duration = Duration::from_secs(120);

fn tiempo_maximo(duracion: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, duracion)
}

/// Traza de cada petición: método, ruta (la plantilla, que anota [`anotar_ruta`]) e id de
/// solicitud; al responder, estado y latencia en nivel `info` (los 5xx, además, en `warn`).
#[derive(Debug, Clone, Copy)]
struct SpanPeticion;

impl MakeSpan<Body> for SpanPeticion {
    fn make_span(&mut self, peticion: &Request<Body>) -> tracing::Span {
        let id = peticion
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");
        tracing::info_span!("http", metodo = %peticion.method(), ruta = tracing::field::Empty, id = %id)
    }
}

pub(crate) fn construir(composicion: &Composicion) -> Router {
    let v1 = Router::new()
        .merge(composicion.identidad.rutas())
        .merge(composicion.negocio.rutas())
        .merge(composicion.auditoria.rutas())
        .merge(composicion.archivos.rutas())
        .merge(composicion.catalogo.rutas())
        .merge(composicion.barberos.rutas())
        .merge(composicion.clientes.rutas())
        .merge(composicion.verificacion.rutas())
        .merge(composicion.agenda.rutas())
        .merge(composicion.notificaciones.rutas())
        .merge(composicion.ventas.rutas())
        .layer(tiempo_maximo(TIEMPO_MAXIMO))
        .merge(
            composicion
                .reportes
                .rutas()
                .layer(tiempo_maximo(TIEMPO_MAXIMO_REPORTES)),
        )
        .layer(from_fn_with_state(composicion.origenes.clone(), verificar_origen));

    // Solo en desarrollo y pruebas: mensajes del WhatsApp simulado, reloj y pasada del worker.
    let v1 = if composicion.rutas_de_pruebas {
        v1.merge(
            composicion
                .notificaciones
                .rutas_de_pruebas()
                .merge(crate::pruebas_e2e::rutas(composicion.clone()))
                .layer(tiempo_maximo(TIEMPO_MAXIMO)),
        )
    } else {
        v1
    };

    let salud = Router::new()
        .route("/api/salud", get(salud::viva))
        .route(
            "/api/salud/lista",
            get(salud::lista).with_state(composicion.pool.clone()),
        )
        .layer(tiempo_maximo(TIEMPO_MAXIMO));

    // Fotos públicas (`/media/publico/*`): el proxy (Traefik) también las enruta a la API.
    let media = composicion
        .archivos
        .rutas_media()
        .layer(tiempo_maximo(TIEMPO_MAXIMO));

    // `anotar_ruta` va en cada enrutador (con `route_layer`): ahí ya se conoce la plantilla
    // completa de la ruta, `/api/v1` incluido.
    let app = Router::new()
        .merge(salud.route_layer(from_fn(anotar_ruta)))
        .nest("/api/v1", v1.route_layer(from_fn(anotar_ruta)))
        .merge(media.route_layer(from_fn(anotar_ruta)))
        .fallback(ruta_no_encontrada);
    let app = match &composicion.limites {
        Some(limites) => app.layer(from_fn_with_state(limites.clone(), limitar_uso)),
        None => app,
    };
    let app = app
        .layer(Extension(composicion.sesiones.clone()))
        .layer(Extension(composicion.confianza_proxy))
        .layer(from_fn(cabeceras_de_seguridad));
    let app = if composicion.https {
        app.layer(from_fn(hsts))
    } else {
        app
    };
    app.layer(
        TraceLayer::new_for_http()
            .make_span_with(SpanPeticion)
            .on_request(())
            .on_response(
                DefaultOnResponse::new()
                    .level(Level::INFO)
                    .latency_unit(LatencyUnit::Millis),
            )
            .on_failure(DefaultOnFailure::new().level(Level::WARN)),
    )
    .layer(PropagateRequestIdLayer::x_request_id())
    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

#[cfg(test)]
mod pruebas {
    use std::str::FromStr;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Method, Request, header};
    use http_body_util::BodyExt;
    use nucleo::RelojSistema;
    use plataforma::configuracion::Configuracion;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tower::ServiceExt;

    use super::*;

    /// Enrutador real con un pool que nunca se conecta: sirve para probar el cableado
    /// sin base de datos.
    fn enrutador() -> Router {
        let opciones = PgConnectOptions::from_str("postgres://nadie@127.0.0.1:1/nada").unwrap();
        let pool = PgPoolOptions::new().connect_lazy_with(opciones);
        let configuracion = Configuracion::desde(|clave| match clave {
            "DATABASE_URL" => Some("postgres://nadie@127.0.0.1:1/nada".into()),
            "DIAMANTE_URL_PUBLICA" => Some("https://diamante.co".into()),
            _ => None,
        })
        .unwrap();
        construir(&Composicion::con_pool(pool, Arc::new(RelojSistema), &configuracion).unwrap())
    }

    async fn pedir(ruta: &str) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        pedir_con(Request::get(ruta).body(Body::empty()).unwrap()).await
    }

    async fn pedir_con(peticion: Request<Body>) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let respuesta = enrutador().oneshot(peticion).await.unwrap();
        let (partes, cuerpo) = respuesta.into_parts();
        let bytes = cuerpo.collect().await.unwrap().to_bytes();
        (
            partes.status,
            partes.headers,
            serde_json::from_slice(&bytes).unwrap_or_default(),
        )
    }

    #[tokio::test]
    async fn la_salud_responde_y_lleva_id_de_solicitud() {
        let (estado, cabeceras, cuerpo) = pedir("/api/salud").await;
        assert_eq!(estado, StatusCode::OK);
        assert_eq!(cuerpo["estado"], "viva");
        assert!(cabeceras.contains_key("x-request-id"));
        // DIAMANTE_URL_PUBLICA es https: el navegador debe quedarse en HTTPS (HSTS).
        assert_eq!(cabeceras[header::STRICT_TRANSPORT_SECURITY], "max-age=31536000");
        assert_eq!(cabeceras[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    #[tokio::test]
    async fn las_rutas_de_los_modulos_exigen_sesion() {
        let (estado, cabeceras, cuerpo) = pedir("/api/v1/negocio").await;
        assert_eq!(estado, StatusCode::UNAUTHORIZED);
        assert_eq!(cabeceras[header::CONTENT_TYPE], "application/problem+json");
        assert_eq!(cuerpo["codigo"], "no_autenticado");
    }

    #[tokio::test]
    async fn rechaza_peticiones_que_modifican_datos_desde_otro_origen() {
        let peticion = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/auth/sesion")
            .header(header::ORIGIN, "https://sitio-malicioso.example")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"correo":"a@b.co","contrasena":"x"}"#))
            .unwrap();
        let (estado, _, cuerpo) = pedir_con(peticion).await;
        assert_eq!(estado, StatusCode::FORBIDDEN);
        assert_eq!(cuerpo["codigo"], "origen_invalido");
    }

    #[tokio::test]
    async fn las_rutas_de_identidad_y_auditoria_estan_montadas() {
        for ruta in [
            "/api/v1/auth/sesion",
            "/api/v1/administradores",
            "/api/v1/auditoria",
        ] {
            let (estado, _, cuerpo) = pedir(ruta).await;
            assert_eq!(estado, StatusCode::UNAUTHORIZED, "{ruta}");
            assert_eq!(cuerpo["codigo"], "no_autenticado");
        }
    }

    #[tokio::test]
    async fn las_rutas_inexistentes_responden_problem_details() {
        for ruta in ["/api/v1/no-existe", "/otra-cosa"] {
            let (estado, _, cuerpo) = pedir(ruta).await;
            assert_eq!(estado, StatusCode::NOT_FOUND, "{ruta}");
            assert_eq!(cuerpo["codigo"], "no_encontrado");
        }
    }
}
