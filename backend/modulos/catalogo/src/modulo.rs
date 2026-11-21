use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::api::{CatalogoServicios, OferentesServicio};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http;
use crate::infraestructura::postgres::RepositorioServiciosPg;

/// Lo que el módulo necesita de afuera.
#[derive(Debug, Clone)]
pub struct DependenciasCatalogo {
    pub pool: PgPool,
    /// Del módulo `auditoria`.
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    /// Del módulo `barberos`.
    pub oferentes: Arc<dyn OferentesServicio<UnidadDeTrabajo>>,
}

#[derive(Debug, Clone)]
pub struct ModuloCatalogo {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloCatalogo {
    pub fn con_postgres(dependencias: DependenciasCatalogo) -> Self {
        Self {
            casos: Arc::new(CasosDeUso::nuevos(Puertos {
                transacciones: Arc::new(TransaccionesPg::nuevas(dependencias.pool)),
                servicios: Arc::new(RepositorioServiciosPg),
                oferentes: dependencias.oferentes,
                auditor: dependencias.auditor,
            })),
        }
    }

    /// Rutas HTTP del módulo, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(self.casos.clone())
    }

    /// Fachada para `barberos` y, más adelante, `agenda` y `ventas`.
    pub fn servicios(&self) -> Arc<dyn CatalogoServicios<UnidadDeTrabajo>> {
        self.casos.fachada.clone()
    }
}
