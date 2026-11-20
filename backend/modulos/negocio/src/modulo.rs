//! Construcción del módulo. La raíz de composición (`app`) le entrega sus dependencias y
//! recibe a cambio sus rutas y su fachada.

use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use nucleo::Reloj;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::api::CalendarioNegocio;
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http;
use crate::infraestructura::postgres::{RepositorioDiasEspecialesPg, RepositorioNegocioPg};

#[derive(Debug, Clone)]
pub struct ModuloNegocio {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloNegocio {
    /// Construye el módulo con sus repositorios de PostgreSQL. Los cambios se auditan con
    /// `auditor` dentro de la misma transacción.
    pub fn con_postgres(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    ) -> Self {
        let casos = CasosDeUso::nuevos(Puertos {
            transacciones: Arc::new(TransaccionesPg::nuevas(pool)),
            negocio: Arc::new(RepositorioNegocioPg),
            dias: Arc::new(RepositorioDiasEspecialesPg),
            auditor,
            reloj,
        });
        Self {
            casos: Arc::new(casos),
        }
    }

    /// Rutas HTTP del módulo, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(self.casos.clone())
    }

    /// Fachada para el módulo `agenda`.
    pub fn calendario(&self) -> Arc<dyn CalendarioNegocio<UnidadDeTrabajo>> {
        self.casos.listar_dias.clone()
    }
}
