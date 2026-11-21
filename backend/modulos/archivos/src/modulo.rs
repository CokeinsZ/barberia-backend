use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use nucleo::Reloj;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use plataforma::seguridad::FirmadorUrls;
use sqlx::PgPool;

use crate::api::Archivos;
use crate::aplicacion::ServicioArchivos;
use crate::http;
use crate::infraestructura::disco::AlmacenamientoEnDisco;
use crate::infraestructura::postgres::RepositorioArchivosPg;

/// Lo que el módulo necesita de afuera.
#[derive(Debug, Clone)]
pub struct DependenciasArchivos {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    /// Carpeta raíz del almacenamiento (`DIAMANTE_ARCHIVOS_DIR`).
    pub raiz: PathBuf,
    pub firmador: FirmadorUrls,
}

#[derive(Debug, Clone)]
pub struct ModuloArchivos {
    servicio: Arc<ServicioArchivos<UnidadDeTrabajo>>,
}

impl ModuloArchivos {
    pub fn con_postgres(dependencias: DependenciasArchivos) -> Self {
        Self {
            servicio: Arc::new(ServicioArchivos {
                transacciones: Arc::new(TransaccionesPg::nuevas(dependencias.pool)),
                repositorio: Arc::new(RepositorioArchivosPg),
                almacenamiento: Arc::new(AlmacenamientoEnDisco::nuevo(dependencias.raiz)),
                firmador: dependencias.firmador,
                reloj: dependencias.reloj,
            }),
        }
    }

    /// Fachada para los módulos dueños de archivos (p. ej. `barberos`).
    pub fn archivos(&self) -> Arc<dyn Archivos<UnidadDeTrabajo>> {
        self.servicio.clone()
    }

    /// `GET /archivos/{id}` (bajo `/api/v1`).
    pub fn rutas(&self) -> Router {
        http::rutas_api(self.servicio.clone())
    }

    /// `GET /media/{*ruta}` (en la raíz). En producción Caddy atiende esta ruta antes.
    pub fn rutas_media(&self) -> Router {
        http::rutas_media(self.servicio.clone())
    }
}
