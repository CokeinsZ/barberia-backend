use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post, put};

use super::Limites;
use super::manejadores as m;
use crate::aplicacion::CasosDeUso;

/// Estado compartido por los manejadores del módulo.
#[derive(Debug)]
pub(crate) struct EstadoHttp<C: Send> {
    pub(crate) casos: Arc<CasosDeUso<C>>,
    pub(crate) limites: Arc<Limites>,
}

impl<C: Send> Clone for EstadoHttp<C> {
    fn clone(&self) -> Self {
        Self {
            casos: self.casos.clone(),
            limites: self.limites.clone(),
        }
    }
}

/// Rutas del módulo, relativas a `/api/v1` (`docs/diseno/api.md`, sección 2.3).
pub(crate) fn rutas<C: Send + 'static>(estado: EstadoHttp<C>) -> Router {
    Router::new()
        .route(
            "/auth/sesion",
            post(m::iniciar_sesion::<C>)
                .get(m::consultar_sesion::<C>)
                .delete(m::cerrar_sesion::<C>),
        )
        .route("/auth/activacion", post(m::activar_cuenta::<C>))
        .route("/auth/recuperacion", post(m::solicitar_recuperacion::<C>))
        .route(
            "/auth/recuperacion/confirmar",
            post(m::restablecer_contrasena::<C>),
        )
        .route("/auth/contrasena", put(m::cambiar_contrasena::<C>))
        .route(
            "/administradores",
            get(m::listar_administradores::<C>).post(m::invitar_administrador::<C>),
        )
        .route(
            "/administradores/{id}",
            patch(m::cambiar_estado_administrador::<C>),
        )
        .with_state(estado)
}
