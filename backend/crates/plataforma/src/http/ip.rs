//! IP del cliente, para los límites de intentos y el registro de sesiones.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

/// Si es `true`, la API está detrás de un proxy de confianza (Caddy) y la IP del cliente es
/// el último valor de `X-Forwarded-For`, el que agregó ese proxy. Se agrega como extensión.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfianzaProxy(pub bool);

#[derive(Debug, Clone, Copy)]
pub struct IpCliente(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for IpCliente {
    type Rejection = Infallible;

    async fn from_request_parts(partes: &mut Parts, _estado: &S) -> Result<Self, Self::Rejection> {
        let confiar = partes.extensions.get::<ConfianzaProxy>().is_some_and(|c| c.0);
        let desde_proxy = confiar
            .then(|| {
                partes
                    .headers
                    .get("x-forwarded-for")?
                    .to_str()
                    .ok()?
                    .rsplit(',')
                    .next()?
                    .trim()
                    .parse()
                    .ok()
            })
            .flatten();
        let directa = partes
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Ok(Self(desde_proxy.or(directa)))
    }
}

#[cfg(test)]
mod pruebas {
    use axum::http::Request;

    use super::*;

    async fn ip(confiar: bool, xff: Option<&str>) -> Option<IpAddr> {
        let mut peticion = Request::builder().uri("/");
        if let Some(valor) = xff {
            peticion = peticion.header("x-forwarded-for", valor);
        }
        let (mut partes, ()) = peticion.body(()).unwrap().into_parts();
        partes.extensions.insert(ConfianzaProxy(confiar));
        partes
            .extensions
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 2], 5000))));
        IpCliente::from_request_parts(&mut partes, &()).await.unwrap().0
    }

    #[tokio::test]
    async fn usa_el_proxy_solo_si_es_de_confianza() {
        let falsa = Some("1.1.1.1, 200.1.2.3");
        assert_eq!(ip(true, falsa).await, "200.1.2.3".parse().ok());
        assert_eq!(ip(false, falsa).await, "10.0.0.2".parse().ok());
        assert_eq!(ip(true, None).await, "10.0.0.2".parse().ok());
    }
}
