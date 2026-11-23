//! Construcción del módulo. La raíz de composición (`app`) le entrega las fachadas de los
//! módulos de los que depende.

use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use barberos::api::EquipoAgenda;
use catalogo::api::CatalogoServicios;
use chrono::{DateTime, Utc};
use clientes::api::ClientesAgenda;
use contratos::barberos::{AusenciaRegistrada, BarberoRetirado, BloqueoCreado, HorarioCambiado};
use contratos::notificaciones::RespuestaWhatsAppRecibida;
use contratos::ventas::{VentaAnulada, VentaRegistrada};
use negocio::api::{CalendarioNegocio, DatosPublicos};
use notificaciones::api::{Avisos, BuzonSalida, ConsultaMensajes};
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;
use verificacion::api::Comprobantes;

use crate::api::{ErrorAgenda, ResumenRecordatorios};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::fachada::FachadaCitas;
use crate::aplicacion::manejadores::{
    AlAnularVenta, AlCambiarDisponibilidad, AlRegistrarVenta, AlResponderWhatsApp,
};
use crate::aplicacion::puertos::Puertos;
use crate::http::{self, Estado};
use crate::infraestructura::postgres::{RepositorioAgendaPg, limpiar};

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
    pub avisos: Arc<dyn Avisos<UnidadDeTrabajo>>,
    pub mensajes: Arc<dyn ConsultaMensajes<UnidadDeTrabajo>>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    pub bus: Arc<BusEventos<UnidadDeTrabajo>>,
    /// URL pública del sitio, para los enlaces de gestión de las citas.
    pub url_publica: String,
}

#[derive(Debug, Clone)]
pub struct ModuloAgenda {
    pool: PgPool,
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
}

impl ModuloAgenda {
    pub fn con_postgres(d: DependenciasAgenda) -> Self {
        Self {
            pool: d.pool.clone(),
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
                avisos: d.avisos,
                mensajes: d.mensajes,
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

    /// Reacciones de la agenda a otros módulos: respuestas a los botones del recordatorio y
    /// avisos cuando un cambio del barbero deja citas por fuera.
    pub fn suscribir(&self, bus: &BusEventos<UnidadDeTrabajo>) {
        let p = &self.casos.p;
        bus.suscribir::<RespuestaWhatsAppRecibida, _>(AlResponderWhatsApp { p: p.clone() });
        bus.suscribir::<HorarioCambiado, _>(AlCambiarDisponibilidad { p: p.clone() });
        bus.suscribir::<BloqueoCreado, _>(AlCambiarDisponibilidad { p: p.clone() });
        bus.suscribir::<AusenciaRegistrada, _>(AlCambiarDisponibilidad { p: p.clone() });
        bus.suscribir::<BarberoRetirado, _>(AlCambiarDisponibilidad { p: p.clone() });
        bus.suscribir::<VentaRegistrada, _>(AlRegistrarVenta { p: p.clone() });
        bus.suscribir::<VentaAnulada, _>(AlAnularVenta { p: p.clone() });
    }

    /// Fachada para `ventas`: la cita que se cobra.
    pub fn citas_para_ventas(&self) -> Arc<dyn crate::api::CitasParaVentas<UnidadDeTrabajo>> {
        Arc::new(FachadaCitas {
            p: self.casos.p.clone(),
        })
    }

    /// Fachada para `clientes` (por un adaptador en `app`): citas por atender de un cliente.
    pub fn citas_de_clientes(&self) -> Arc<dyn crate::api::CitasDeClientes<UnidadDeTrabajo>> {
        Arc::new(FachadaCitas {
            p: self.casos.p.clone(),
        })
    }

    /// Tarea periódica del worker: recordatorios de 24 h y 2 h (RF-NOT-02).
    pub async fn enviar_recordatorios(&self) -> Result<ResumenRecordatorios, ErrorAgenda> {
        self.casos.recordatorios.enviar_debidos().await
    }

    /// Mantenimiento: borra los enlaces de gestión vencidos.
    pub async fn limpiar(&self, ahora: DateTime<Utc>) -> Result<u64, ErrorAgenda> {
        Ok(limpiar(&self.pool, ahora).await?)
    }
}
