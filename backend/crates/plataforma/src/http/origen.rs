//! Verificación de `Origin` en las peticiones que modifican datos (defensa contra CSRF,
//! además de `SameSite=Strict`). Si el navegador envía `Origin` y no es uno permitido, 403.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::ErrorHttp;

#[derive(Debug, Clone)]
pub struct OrigenesPermitidos(pub Arc<Vec<String>>);

pub async fn verificar_origen(
    State(permitidos): State<OrigenesPermitidos>,
    peticion: Request,
    siguiente: Next,
) -> Response {
    let segura = matches!(*peticion.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !segura && let Some(origen) = peticion.headers().get(header::ORIGIN) {
        let permitido = origen.to_str().is_ok_and(|o| permitidos.0.iter().any(|p| p == o));
        if !permitido {
            return ErrorHttp::nuevo(
                StatusCode::FORBIDDEN,
                "origen_invalido",
                "La petición viene de un origen no permitido.",
            )
            .into_response();
        }
    }
    siguiente.run(peticion).await
}

#[cfg(test)]
mod pruebas {
    use axum::Router;
    use axum::body::Body;
    use axum::middleware::from_fn_with_state;
    use axum::routing::post;
    use tower::ServiceExt;

    use super::*;

    async fn estado(metodo: Method, origen: Option<&str>) -> StatusCode {
        let permitidos = OrigenesPermitidos(Arc::new(vec!["https://diamante.co".into()]));
        let app = Router::new()
            .route("/x", post(|| async { "ok" }).get(|| async { "ok" }))
            .layer(from_fn_with_state(permitidos, verificar_origen));
        let mut peticion = Request::builder().method(metodo).uri("/x");
        if let Some(o) = origen {
            peticion = peticion.header(header::ORIGIN, o);
        }
        app.oneshot(peticion.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn rechaza_origenes_ajenos_solo_al_modificar() {
        assert_eq!(
            estado(Method::POST, Some("https://diamante.co")).await,
            StatusCode::OK
        );
        assert_eq!(
            estado(Method::POST, Some("https://ataque.example")).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(estado(Method::POST, None).await, StatusCode::OK);
        assert_eq!(
            estado(Method::GET, Some("https://ataque.example")).await,
            StatusCode::OK
        );
    }
}
