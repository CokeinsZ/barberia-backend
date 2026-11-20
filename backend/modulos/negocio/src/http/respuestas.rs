use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Respuesta `201 Created` con la ubicación del recurso nuevo.
pub(crate) struct Creado<T> {
    pub(crate) ubicacion: String,
    pub(crate) cuerpo: T,
}

impl<T: Serialize> IntoResponse for Creado<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::CREATED,
            [(header::LOCATION, self.ubicacion)],
            Json(self.cuerpo),
        )
            .into_response()
    }
}
