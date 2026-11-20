use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get};

use super::manejadores;
use crate::aplicacion::CasosDeUso;

/// Rutas del módulo (relativas a `/api/v1`). Todas exigen sesión (ver `docs/diseno/api.md`, 2.4).
pub(crate) fn rutas(casos: Arc<CasosDeUso>) -> Router {
    Router::new()
        .route(
            "/negocio",
            get(manejadores::obtener_datos).put(manejadores::actualizar_datos),
        )
        .route(
            "/negocio/dias-especiales",
            get(manejadores::listar_dias).post(manejadores::registrar_dia),
        )
        .route("/negocio/dias-especiales/{id}", delete(manejadores::eliminar_dia))
        .with_state(casos)
}
