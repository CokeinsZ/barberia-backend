//! Construcción del módulo. La raíz de composición (`app`) le entrega sus dependencias y
//! recibe a cambio sus rutas y su fachada.

use std::sync::Arc;

use axum::Router;
use nucleo::Reloj;
use sqlx::PgPool;

use crate::api::CalendarioNegocio;
use crate::aplicacion::CasosDeUso;
use crate::http;
use crate::infraestructura::postgres::{RepositorioDiasEspecialesPg, RepositorioNegocioPg};

#[derive(Debug, Clone)]
pub struct ModuloNegocio {
    casos: Arc<CasosDeUso>,
}

impl ModuloNegocio {
    /// Construye el módulo con sus repositorios de PostgreSQL.
    pub fn con_postgres(pool: PgPool, reloj: Arc<dyn Reloj>) -> Self {
        let casos = CasosDeUso::nuevos(
            Arc::new(RepositorioNegocioPg::nuevo(pool.clone())),
            Arc::new(RepositorioDiasEspecialesPg::nuevo(pool)),
            reloj,
        );
        Self {
            casos: Arc::new(casos),
        }
    }

    /// Rutas HTTP del módulo, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(self.casos.clone())
    }

    /// Fachada para el módulo `agenda`.
    pub fn calendario(&self) -> Arc<dyn CalendarioNegocio> {
        self.casos.listar_dias.clone()
    }
}
