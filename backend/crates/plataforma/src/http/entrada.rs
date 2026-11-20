use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::ErrorHttp;

/// Cuerpo JSON de la petición. A diferencia de `Json`, si el cuerpo no sirve responde con
/// *Problem Details* (400) en lugar de texto plano.
#[derive(Debug, Clone)]
pub struct Entrada<T>(pub T);

impl<T, S> FromRequest<S> for Entrada<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErrorHttp;

    async fn from_request(peticion: Request, estado: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(peticion, estado).await {
            Ok(Json(valor)) => Ok(Self(valor)),
            Err(JsonRejection::MissingJsonContentType(_)) => Err(ErrorHttp::solicitud_invalida(
                "El cuerpo debe enviarse como JSON (Content-Type: application/json).",
            )),
            Err(JsonRejection::JsonSyntaxError(_)) => {
                Err(ErrorHttp::solicitud_invalida("El JSON está mal formado."))
            }
            Err(otro) => Err(ErrorHttp::solicitud_invalida(otro.body_text())),
        }
    }
}

/// Parámetros de la URL (`?desde=…&hasta=…`) con errores en formato *Problem Details*.
#[derive(Debug, Clone)]
pub struct Parametros<T>(pub T);

impl<T, S> FromRequestParts<S> for Parametros<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErrorHttp;

    async fn from_request_parts(partes: &mut Parts, estado: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(partes, estado)
            .await
            .map(|Query(valor)| Self(valor))
            .map_err(|rechazo| ErrorHttp::solicitud_invalida(rechazo.body_text()))
    }
}
