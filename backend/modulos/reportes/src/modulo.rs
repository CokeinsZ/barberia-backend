//! Construcción del módulo. La raíz de composición (`app`) le entrega las fachadas que usa.

use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use barberos::api::EquipoAgenda;
use clientes::api::ClientesAgenda;
use negocio::api::{CalendarioNegocio, ParametrosNegocio};
use notificaciones::api::Avisos;
use nucleo::Reloj;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use sqlx::PgPool;

use crate::aplicacion::casos::Reportes;
use crate::aplicacion::puertos::{GeneradorPdf, Puertos};
use crate::infraestructura::gotenberg::Gotenberg;
use crate::infraestructura::postgres::LecturasPg;

#[derive(Debug, Clone)]
pub struct DependenciasReportes {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    pub equipo: Arc<dyn EquipoAgenda<UnidadDeTrabajo>>,
    pub calendario: Arc<dyn CalendarioNegocio<UnidadDeTrabajo>>,
    pub negocio: Arc<dyn ParametrosNegocio<UnidadDeTrabajo>>,
    pub clientes: Arc<dyn ClientesAgenda<UnidadDeTrabajo>>,
    pub avisos: Arc<dyn Avisos<UnidadDeTrabajo>>,
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    /// URL de Gotenberg (`http://gotenberg:3000`). Sin ella, los PDF responden 503.
    pub gotenberg_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModuloReportes {
    reportes: Arc<Reportes<UnidadDeTrabajo>>,
}

impl ModuloReportes {
    pub fn con_postgres(d: DependenciasReportes) -> Self {
        let pdf: Option<Arc<dyn GeneradorPdf>> =
            d.gotenberg_url
                .as_deref()
                .and_then(|url| match Gotenberg::nuevo(url) {
                    Ok(g) => Some(Arc::new(g) as Arc<dyn GeneradorPdf>),
                    Err(e) => {
                        tracing::error!(error = %e, "no se pudo preparar el cliente de Gotenberg");
                        None
                    }
                });
        Self {
            reportes: Arc::new(Reportes {
                p: Arc::new(Puertos {
                    transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                    lecturas: Arc::new(LecturasPg),
                    equipo: d.equipo,
                    calendario: d.calendario,
                    negocio: d.negocio,
                    clientes: d.clientes,
                    avisos: d.avisos,
                    auditor: d.auditor,
                    pdf,
                    reloj: d.reloj,
                }),
            }),
        }
    }

    /// Rutas del panel, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        crate::http::rutas(self.reportes.clone())
    }

    /// `true` si hay generador de PDF configurado.
    pub fn genera_pdf(&self) -> bool {
        self.reportes.p.pdf.is_some()
    }
}
