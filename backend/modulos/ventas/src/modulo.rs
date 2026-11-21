//! Construcción del módulo. La raíz de composición (`app`) le entrega las fachadas de los
//! módulos de los que depende.

use std::sync::Arc;

use agenda::api::CitasParaVentas;
use auditoria::api::Auditor;
use axum::Router;
use barberos::api::EquipoVentas;
use catalogo::api::CatalogoServicios;
use clientes::api::ClientesAgenda;
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::infraestructura::postgres::RepositorioVentasPg;

#[derive(Debug, Clone)]
pub struct DependenciasVentas {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    pub citas: Arc<dyn CitasParaVentas<UnidadDeTrabajo>>,
    pub equipo: Arc<dyn EquipoVentas<UnidadDeTrabajo>>,
    pub catalogo: Arc<dyn CatalogoServicios<UnidadDeTrabajo>>,
    pub clientes: Arc<dyn ClientesAgenda<UnidadDeTrabajo>>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    /// `VentaRegistrada` y `VentaAnulada`: la agenda y los clientes reaccionan en la misma
    /// transacción.
    pub bus: Arc<BusEventos<UnidadDeTrabajo>>,
}

#[derive(Debug, Clone)]
pub struct ModuloVentas {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloVentas {
    pub fn con_postgres(d: DependenciasVentas) -> Self {
        Self {
            casos: Arc::new(CasosDeUso::nuevos(Puertos {
                transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                ventas: Arc::new(RepositorioVentasPg),
                citas: d.citas,
                equipo: d.equipo,
                catalogo: d.catalogo,
                clientes: d.clientes,
                auditor: d.auditor,
                bus: d.bus,
                reloj: d.reloj,
            })),
        }
    }

    /// Rutas del panel, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        crate::http::rutas(self.casos.clone())
    }
}
