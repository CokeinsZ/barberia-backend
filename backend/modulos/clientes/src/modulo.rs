use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use contratos::agenda::InasistenciaMarcada;
use contratos::ventas::{VentaAnulada, VentaRegistrada};
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::api::{AgendaDelCliente, ClientesAgenda};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http;
use crate::infraestructura::eventos::{ProyeccionInasistencias, ProyeccionVentas};
use crate::infraestructura::postgres::RepositorioClientesPg;

#[derive(Debug, Clone)]
pub struct DependenciasClientes {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    /// Puerto que implementa `app` con la agenda (citas por atender antes de anonimizar).
    pub agenda: Arc<dyn AgendaDelCliente<UnidadDeTrabajo>>,
}

#[derive(Debug, Clone)]
pub struct ModuloClientes {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloClientes {
    pub fn con_postgres(d: DependenciasClientes) -> Self {
        Self {
            casos: Arc::new(CasosDeUso::nuevos(Puertos {
                transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                clientes: Arc::new(RepositorioClientesPg),
                titular: Arc::new(RepositorioClientesPg),
                agenda: d.agenda,
                auditor: d.auditor,
                reloj: d.reloj,
            })),
        }
    }

    /// Suscribe la proyección de métricas a los eventos de otros módulos (misma transacción).
    pub fn suscribir(bus: &BusEventos<UnidadDeTrabajo>) {
        bus.suscribir::<InasistenciaMarcada, _>(ProyeccionInasistencias);
        bus.suscribir::<VentaRegistrada, _>(ProyeccionVentas);
        bus.suscribir::<VentaAnulada, _>(ProyeccionVentas);
    }

    pub fn rutas(&self) -> Router {
        http::rutas(self.casos.clone())
    }

    /// Fachada para `agenda` (y, por un adaptador en `app`, para `verificacion`).
    pub fn clientes(&self) -> Arc<dyn ClientesAgenda<UnidadDeTrabajo>> {
        self.casos.fachada.clone()
    }
}
