//! Sesión del personal. Toda ruta que use [`ActorAutenticado`] exige una sesión válida:
//! el sistema es seguro por defecto y solo las rutas públicas omiten este extractor.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use nucleo::Actor;

use super::ErrorHttp;

/// Cookie de la sesión (`HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/`).
pub const COOKIE_SESION: &str = "__Host-sesion";

/// Valida un token de sesión. Lo implementa el módulo `identidad`; mientras no exista,
/// la raíz de composición usa [`SinSesiones`], que no acepta ninguna.
#[async_trait]
pub trait ValidadorSesion: Send + Sync + 'static {
    async fn validar(&self, token: &str) -> Result<Option<Actor>, ErrorHttp>;
}

/// Validador que rechaza todas las sesiones.
#[derive(Debug, Default, Clone, Copy)]
pub struct SinSesiones;

#[async_trait]
impl ValidadorSesion for SinSesiones {
    async fn validar(&self, _token: &str) -> Result<Option<Actor>, ErrorHttp> {
        Ok(None)
    }
}

/// Validador disponible para los extractores. Se agrega al enrutador como extensión.
#[derive(Clone)]
pub struct Sesiones(pub Arc<dyn ValidadorSesion>);

impl fmt::Debug for Sesiones {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sesiones")
    }
}

/// Actor de la petición, tomado de la cookie de sesión. Si no hay sesión válida, responde 401.
#[derive(Debug, Clone)]
pub struct ActorAutenticado(pub Actor);

impl<S: Send + Sync> FromRequestParts<S> for ActorAutenticado {
    type Rejection = ErrorHttp;

    async fn from_request_parts(partes: &mut Parts, _estado: &S) -> Result<Self, Self::Rejection> {
        let Some(sesiones) = partes.extensions.get::<Sesiones>().cloned() else {
            tracing::error!("el enrutador no tiene configurado el validador de sesiones");
            return Err(ErrorHttp::nuevo(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error_interno",
                "Error de configuración.",
            ));
        };
        let token = leer_cookie(&partes.headers, COOKIE_SESION).ok_or_else(ErrorHttp::no_autenticado)?;
        match sesiones.0.validar(&token).await? {
            Some(actor) => Ok(Self(actor)),
            None => Err(ErrorHttp::no_autenticado()),
        }
    }
}

pub(crate) fn leer_cookie(cabeceras: &HeaderMap, nombre: &str) -> Option<String> {
    cabeceras
        .get_all(COOKIE)
        .iter()
        .filter_map(|valor| valor.to_str().ok())
        .flat_map(|valor| valor.split(';'))
        .filter_map(|par| par.trim().split_once('='))
        .find(|(clave, valor)| *clave == nombre && !valor.is_empty())
        .map(|(_, valor)| valor.to_owned())
}

#[cfg(test)]
mod pruebas {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn lee_la_cookie_entre_varias() {
        let mut cabeceras = HeaderMap::new();
        cabeceras.append(
            COOKIE,
            HeaderValue::from_static("tema=oscuro; __Host-sesion=abc123"),
        );
        cabeceras.append(COOKIE, HeaderValue::from_static("otra=1"));
        assert_eq!(leer_cookie(&cabeceras, COOKIE_SESION).as_deref(), Some("abc123"));
        assert_eq!(leer_cookie(&cabeceras, "no_existe"), None);
    }

    #[test]
    fn ignora_la_cookie_vacia() {
        let mut cabeceras = HeaderMap::new();
        cabeceras.append(COOKIE, HeaderValue::from_static("__Host-sesion="));
        assert_eq!(leer_cookie(&cabeceras, COOKIE_SESION), None);
    }
}
