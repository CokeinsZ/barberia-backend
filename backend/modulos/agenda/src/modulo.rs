//! Construcción del módulo. La raíz de composición (`app`) le entrega las fachadas de los
//! módulos de los que depende.

use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use barberos::api::EquipoAgenda;
use catalogo::api::CatalogoServicios;
use clientes::api::ClientesAgenda;
use negocio::api::{CalendarioNegocio, DatosPublicos};
use notificaciones::api::BuzonSalida;
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;
use verificacion::api::Comprobantes;

use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http::{self, Estado};
use crate::infraestructura::postgres::RepositorioAgendaPg;

#[derive(Debug, Clone)]
pub struct DependenciasAgenda {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    pub equipo: Arc<dyn EquipoAgenda<UnidadDeTrabajo>>,
    pub catalogo: Arc<dyn CatalogoServicios<UnidadDeTrabajo>>,
    pub calendario: Arc<dyn CalendarioNegocio<UnidadDeTrabajo>>,
    pub negocio: Arc<dyn DatosPublicos<UnidadDeTrabajo>>,
    pub clientes: Arc<dyn ClientesAgenda<UnidadDeTrabajo>>,
    pub comprobantes: Arc<dyn Comprobantes<UnidadDeTrabajo>>,
    pub buzon: Arc<dyn BuzonSalida<UnidadDeTrabajo>>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    pub bus: Arc<BusEventos<UnidadDeTrabajo>>,
    /// URL pública del sitio, para los enlaces de gestión de las citas.
    pub url_publica: String,
}

#[derive(Debug, Clone)]
pub struct ModuloAgenda {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloAgenda {
    pub fn con_postgres(d: DependenciasAgenda) -> Self {
        Self {
            casos: Arc::new(CasosDeUso::nuevos(Puertos {
                transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                citas: Arc::new(RepositorioAgendaPg),
                equipo: d.equipo,
                catalogo: d.catalogo,
                calendario: d.calendario,
                negocio: d.negocio,
                clientes: d.clientes,
                comprobantes: d.comprobantes,
                buzon: d.buzon,
                bus: d.bus,
                auditor: d.auditor,
                reloj: d.reloj,
                url_publica: d.url_publica,
            })),
        }
    }

    /// Rutas públicas y del panel, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(Estado::nuevo(self.casos.clone()))
    }
}
