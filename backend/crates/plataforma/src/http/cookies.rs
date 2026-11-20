//! Cookie de sesión: `__Host-sesion`, `HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/`.
//! El prefijo `__Host-` obliga al navegador a exigir `Secure`, `Path=/` y ningún `Domain`.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::HeaderValue;
use axum::http::request::Parts;

use super::ErrorHttp;
use super::sesion::{COOKIE_SESION, leer_cookie};

/// Valor de `Set-Cookie` para una sesión. Sin duración, la cookie vive hasta cerrar el navegador.
pub fn cookie_sesion(token: &str, duracion: Option<Duration>) -> Result<HeaderValue, ErrorHttp> {
    let mut valor = format!("{COOKIE_SESION}={token}; Path=/; HttpOnly; Secure; SameSite=Strict");
    if let Some(duracion) = duracion {
        valor.push_str(&format!("; Max-Age={}", duracion.as_secs()));
    }
    HeaderValue::from_str(&valor).map_err(|_| ErrorHttp::interno())
}

/// Valor de `Set-Cookie` que borra la sesión del navegador.
pub fn cookie_sesion_vencida() -> HeaderValue {
    HeaderValue::from_static("__Host-sesion=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

/// Token de la cookie de sesión, si viene (sin validarlo). Útil para cerrar la sesión.
#[derive(Debug, Clone)]
pub struct TokenSesion(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for TokenSesion {
    type Rejection = Infallible;

    async fn from_request_parts(partes: &mut Parts, _estado: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(leer_cookie(&partes.headers, COOKIE_SESION)))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn arma_la_cookie_con_atributos_seguros() {
        let cookie = cookie_sesion("abc", Some(Duration::from_secs(60))).unwrap();
        assert_eq!(
            cookie,
            "__Host-sesion=abc; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=60"
        );
        assert!(
            cookie_sesion("abc", None)
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("SameSite=Strict")
        );
    }
}
