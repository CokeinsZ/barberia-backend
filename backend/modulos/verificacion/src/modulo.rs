use std::sync::Arc;

use axum::Router;
use notificaciones::api::BuzonSalida;
use nucleo::Reloj;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use plataforma::seguridad::FirmadorUrls;
use sqlx::PgPool;

use crate::api::{ClientesConocidos, Comprobantes};
use crate::aplicacion::casos::Verificacion;
use crate::aplicacion::puertos::Puertos;
use crate::http::{self, Estado, Limites};
use crate::infraestructura::postgres::RepositorioCodigosPg;

#[derive(Debug, Clone)]
pub struct DependenciasVerificacion {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    pub buzon: Arc<dyn BuzonSalida<UnidadDeTrabajo>>,
    /// Adaptador de `app` sobre el módulo `clientes`.
    pub clientes: Arc<dyn ClientesConocidos<UnidadDeTrabajo>>,
    /// Clave del servidor para el HMAC de los códigos.
    pub firmador: FirmadorUrls,
}

#[derive(Debug, Clone)]
pub struct ModuloVerificacion {
    verificacion: Arc<Verificacion<UnidadDeTrabajo>>,
    limites: Arc<Limites>,
}

impl ModuloVerificacion {
    pub fn con_postgres(d: DependenciasVerificacion) -> Self {
        Self {
            verificacion: Arc::new(Verificacion {
                p: Arc::new(Puertos {
                    transacciones: Arc::new(TransaccionesPg::nuevas(d.pool)),
                    codigos: Arc::new(RepositorioCodigosPg),
                    buzon: d.buzon,
                    clientes: d.clientes,
                    firmador: d.firmador,
                    reloj: d.reloj,
                }),
            }),
            limites: Arc::new(Limites::estandar()),
        }
    }

    pub fn rutas(&self) -> Router {
        http::rutas(Estado {
            verificacion: self.verificacion.clone(),
            limites: self.limites.clone(),
        })
    }

    /// Fachada para `agenda`.
    pub fn comprobantes(&self) -> Arc<dyn Comprobantes<UnidadDeTrabajo>> {
        self.verificacion.clone()
    }
}
