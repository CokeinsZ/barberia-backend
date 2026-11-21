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
    pool: sqlx::PgPool,
    verificacion: Arc<Verificacion<UnidadDeTrabajo>>,
    limites: Arc<Limites>,
}

impl ModuloVerificacion {
    pub fn con_postgres(d: DependenciasVerificacion) -> Self {
        Self {
            pool: d.pool.clone(),
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

    /// Mantenimiento nocturno: borra los códigos vencidos hace más de 7 días.
    pub async fn limpiar(
        &self,
        ahora: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, nucleo::ErrorPersistencia> {
        crate::infraestructura::postgres::limpiar(&self.pool, ahora).await
    }

    /// Fachada para `agenda`.
    pub fn comprobantes(&self) -> Arc<dyn Comprobantes<UnidadDeTrabajo>> {
        self.verificacion.clone()
    }
}
