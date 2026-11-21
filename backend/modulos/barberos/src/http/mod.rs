//! Rutas HTTP del módulo (relativas a `/api/v1`, `docs/diseno/api.md`, sección 2.6).

mod manejadores;
mod solicitudes;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};

use crate::aplicacion::CasosDeUso;

/// Límite del cuerpo en las cargas: el archivo más el resto del formulario.
const LIMITE_FOTO: usize = 6 * 1024 * 1024;
const LIMITE_DOCUMENTO: usize = 11 * 1024 * 1024;

pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    use manejadores as m;
    Router::new()
        .route("/yo", get(m::yo::<C>))
        .route("/barberos", get(m::listar::<C>).post(m::registrar::<C>))
        .route("/barberos/orden", put(m::ordenar::<C>))
        .route("/barberos/{id}", get(m::ficha::<C>).put(m::editar::<C>))
        .route("/barberos/{id}/visibilidad", put(m::visibilidad::<C>))
        .route("/barberos/{id}/retiro", post(m::retirar::<C>))
        .route(
            "/barberos/{id}/horario",
            get(m::horario::<C>).put(m::cambiar_horario::<C>),
        )
        .route(
            "/barberos/{id}/servicios",
            get(m::servicios::<C>).put(m::cambiar_servicios::<C>),
        )
        .route(
            "/barberos/{id}/datos-privados",
            get(m::privados::<C>).put(m::guardar_privados::<C>),
        )
        .route(
            "/barberos/{id}/foto",
            post(m::foto::<C>).layer(DefaultBodyLimit::max(LIMITE_FOTO)),
        )
        .route(
            "/barberos/{id}/documentos",
            get(m::documentos::<C>)
                .post(m::cargar_documento::<C>)
                .layer(DefaultBodyLimit::max(LIMITE_DOCUMENTO)),
        )
        .route(
            "/barberos/{id}/documentos/{documento}",
            delete(m::quitar_documento::<C>),
        )
        .route(
            "/barberos/{id}/documentos/{documento}/descarga",
            get(m::descargar_documento::<C>),
        )
        .route(
            "/barberos/{id}/ausencias",
            get(m::ausencias::<C>).post(m::registrar_ausencia::<C>),
        )
        .route(
            "/barberos/{id}/ausencias/{ausencia}/anular",
            post(m::anular_ausencia::<C>),
        )
        .route(
            "/barberos/{id}/ausencias/{ausencia}/soporte",
            post(m::soporte_ausencia::<C>).layer(DefaultBodyLimit::max(LIMITE_DOCUMENTO)),
        )
        .route(
            "/barberos/{id}/bloqueos",
            get(m::bloqueos::<C>).post(m::crear_bloqueo::<C>),
        )
        .route(
            "/barberos/{id}/bloqueos/{bloqueo}",
            delete(m::eliminar_bloqueo::<C>),
        )
        .with_state(casos)
}
