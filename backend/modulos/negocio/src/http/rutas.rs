use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};

use super::manejadores as m;
use crate::aplicacion::CasosDeUso;

/// Rutas del módulo (relativas a `/api/v1`). Todas exigen sesión (ver `docs/diseno/api.md`, 2.4).
pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    Router::new()
        .route(
            "/negocio",
            get(m::obtener_datos::<C>).put(m::actualizar_datos::<C>),
        )
        .route("/negocio/horario-atencion", get(m::horario_atencion::<C>))
        .route(
            "/negocio/dias-especiales",
            get(m::listar_dias::<C>).post(m::registrar_dia::<C>),
        )
        .route(
            "/negocio/dias-especiales/{id}",
            put(m::editar_dia::<C>).delete(m::eliminar_dia::<C>),
        )
        .route(
            "/negocio/dias-especiales/festivos/{anio}",
            post(m::precargar_festivos::<C>),
        )
        .route(
            "/negocio/politicas",
            get(m::listar_politicas::<C>).post(m::publicar_politica::<C>),
        )
        .with_state(casos)
}
