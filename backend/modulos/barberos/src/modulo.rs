//! Construcción del módulo. La raíz de composición (`app`) le entrega sus dependencias y
//! recibe a cambio sus rutas y los adaptadores de los puertos que definen otros módulos.

use std::sync::Arc;

use archivos::api::Archivos;
use auditoria::api::Auditor;
use axum::Router;
use catalogo::api::{CatalogoServicios, OferentesServicio};
use identidad::api::{CuentasPersonal, PerfilesBarberos};
use negocio::api::ParametrosNegocio;
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use plataforma::seguridad::Cifrador;
use sqlx::PgPool;

use crate::api::EquipoAgenda;
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http;
use crate::infraestructura::postgres::{
    OferentesServicioPg, PerfilesBarberosPg, RepositorioBarberosPg, RepositorioExpedientePg,
    RepositorioNovedadesPg,
};

/// Lo que el módulo necesita de afuera.
#[derive(Debug, Clone)]
pub struct DependenciasBarberos {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    /// De `identidad`: cuentas del panel.
    pub cuentas: Arc<dyn CuentasPersonal<UnidadDeTrabajo>>,
    /// De `catalogo`: servicios que puede ofrecer.
    pub catalogo: Arc<dyn CatalogoServicios<UnidadDeTrabajo>>,
    /// De `archivos`: foto, documentos y soportes.
    pub archivos: Arc<dyn Archivos<UnidadDeTrabajo>>,
    /// De `negocio`: comisión por defecto.
    pub parametros: Arc<dyn ParametrosNegocio<UnidadDeTrabajo>>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    pub bus: Arc<BusEventos<UnidadDeTrabajo>>,
    /// Cifra el grupo sanguíneo y el número de cuenta (RNF-LEG-04).
    pub cifrador: Cifrador,
}

#[derive(Debug, Clone)]
pub struct ModuloBarberos {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloBarberos {
    pub fn con_postgres(d: DependenciasBarberos) -> Self {
        Self {
            casos: Arc::new(CasosDeUso::nuevos(Puertos {
                transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                barberos: Arc::new(RepositorioBarberosPg),
                novedades: Arc::new(RepositorioNovedadesPg),
                expediente: Arc::new(RepositorioExpedientePg),
                cuentas: d.cuentas,
                catalogo: d.catalogo,
                archivos: d.archivos,
                parametros: d.parametros,
                auditor: d.auditor,
                bus: d.bus,
                cifrador: d.cifrador,
                reloj: d.reloj,
            })),
        }
    }

    /// Rutas HTTP del módulo, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(self.casos.clone())
    }

    /// Fachada para `agenda`: el equipo con su horario, ausencias y bloqueos.
    pub fn equipo(&self) -> Arc<dyn EquipoAgenda<UnidadDeTrabajo>> {
        self.casos.equipo.clone()
    }

    /// Puerto de `identidad`: quién es el barbero de una cuenta y si puede entrar hoy. Se crea
    /// sin el módulo porque `identidad` se construye antes que `barberos`.
    pub fn perfiles_para_identidad(reloj: Arc<dyn Reloj>) -> Arc<dyn PerfilesBarberos<UnidadDeTrabajo>> {
        Arc::new(PerfilesBarberosPg::nuevo(reloj))
    }

    /// Fachada para `ventas`: estado del barbero en la fecha, sus servicios y su comisión.
    pub fn equipo_para_ventas(&self) -> Arc<dyn crate::api::EquipoVentas<UnidadDeTrabajo>> {
        Arc::new(crate::infraestructura::postgres::EquipoVentasPg)
    }

    /// Puerto de `catalogo`: cuántos barberos vigentes ofrecen cada servicio.
    pub fn oferentes_para_catalogo(reloj: Arc<dyn Reloj>) -> Arc<dyn OferentesServicio<UnidadDeTrabajo>> {
        Arc::new(OferentesServicioPg::nuevo(reloj))
    }
}
