use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get};

use super::manejadores;
use crate::aplicacion::CasosDeUso;

/// Rutas del módulo (relativas a `/api/v1`). Todas exigen sesión (ver `docs/diseno/api.md`, 2.4).
pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    Router::new()
        .route(
            "/negocio",
            get(manejadores::obtener_datos::<C>).put(manejadores::actualizar_datos::<C>),
        )
        .route(
            "/negocio/dias-especiales",
            get(manejadores::listar_dias::<C>).post(manejadores::registrar_dia::<C>),
        )
        .route(
            "/negocio/dias-especiales/{id}",
            delete(manejadores::eliminar_dia::<C>),
        )
        .with_state(casos)
}
