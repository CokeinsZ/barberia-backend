use std::sync::Arc;

use axum::Router;
use nucleo::Reloj;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::api::Auditor;
use crate::aplicacion::ListarRegistros;
use crate::http;
use crate::infraestructura::postgres::{AuditorPg, RepositorioRegistrosPg};

#[derive(Debug, Clone)]
pub struct ModuloAuditoria {
    auditor: Arc<AuditorPg>,
    listar: Arc<ListarRegistros<UnidadDeTrabajo>>,
}

impl ModuloAuditoria {
    pub fn con_postgres(pool: PgPool, reloj: Arc<dyn Reloj>) -> Self {
        Self {
            auditor: Arc::new(AuditorPg::nuevo(reloj)),
            listar: Arc::new(ListarRegistros::nuevo(
                Arc::new(TransaccionesPg::nuevas(pool)),
                Arc::new(RepositorioRegistrosPg),
            )),
        }
    }

    /// Auditor para los demás módulos: escribe dentro de su unidad de trabajo.
    pub fn auditor(&self) -> Arc<dyn Auditor<UnidadDeTrabajo>> {
        self.auditor.clone()
    }

    pub fn rutas(&self) -> Router {
        http::rutas(self.listar.clone())
    }
}
