//! Enrutador HTTP: monta `/api/v1` una sola vez con las rutas de cada módulo y agrega las
//! capas transversales (id de solicitud, trazas, tiempo máximo y sesión).

use std::time::Duration;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Router};
use plataforma::http::ruta_no_encontrada;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::composicion::Composicion;
use crate::salud;

pub(crate) fn construir(composicion: &Composicion) -> Router {
    let v1 = Router::new().merge(composicion.negocio.rutas());

    Router::new()
        .route("/api/salud", get(salud::viva))
        .route(
            "/api/salud/lista",
            get(salud::lista).with_state(composicion.pool.clone()),
        )
        .nest("/api/v1", v1)
        .fallback(ruta_no_encontrada)
        .layer(Extension(composicion.sesiones.clone()))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

#[cfg(test)]
mod pruebas {
    use std::str::FromStr;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, header};
    use http_body_util::BodyExt;
    use nucleo::RelojSistema;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tower::ServiceExt;

    use super::*;

    /// Enrutador real con un pool que nunca se conecta: sirve para probar el cableado
    /// sin base de datos.
    fn enrutador() -> Router {
        let opciones = PgConnectOptions::from_str("postgres://nadie@127.0.0.1:1/nada").unwrap();
        let pool = PgPoolOptions::new().connect_lazy_with(opciones);
        construir(&Composicion::con_pool(pool, Arc::new(RelojSistema)))
    }

    async fn pedir(ruta: &str) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let respuesta = enrutador()
            .oneshot(Request::get(ruta).body(Body::empty()).unwrap())
            .await
            .unwrap();
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
    }

    #[tokio::test]
    async fn las_rutas_de_los_modulos_exigen_sesion() {
        let (estado, cabeceras, cuerpo) = pedir("/api/v1/negocio").await;
        assert_eq!(estado, StatusCode::UNAUTHORIZED);
        assert_eq!(cabeceras[header::CONTENT_TYPE], "application/problem+json");
        assert_eq!(cuerpo["codigo"], "no_autenticado");
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
