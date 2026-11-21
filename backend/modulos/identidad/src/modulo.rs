//! Construcción del módulo. La raíz de composición (`app`) le entrega sus dependencias y
//! recibe a cambio sus rutas, el validador de sesiones y su fachada.

use std::sync::Arc;

use auditoria::api::Auditor;
use axum::Router;
use notificaciones::api::BuzonSalida;
use nucleo::{Actor, Reloj};
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use plataforma::http::ValidadorSesion;
use sqlx::PgPool;

use crate::api::{CuentasPersonal, ErrorIdentidad, InvitacionEmitida, PerfilesBarberos};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::puertos::Puertos;
use crate::http::{self, EstadoHttp, Limites, ValidadorIdentidad};
use crate::infraestructura::postgres::{RepositorioSesionesPg, RepositorioTokensPg, RepositorioUsuariosPg};
use crate::infraestructura::seguridad::CredencialesArgon2;

/// Lo que el módulo necesita de afuera.
#[derive(Debug, Clone)]
pub struct DependenciasIdentidad {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    /// Del módulo `barberos`.
    pub perfiles: Arc<dyn PerfilesBarberos<UnidadDeTrabajo>>,
    /// Del módulo `auditoria`.
    pub auditor: Arc<dyn Auditor<UnidadDeTrabajo>>,
    /// Del módulo `notificaciones`.
    pub buzon: Arc<dyn BuzonSalida<UnidadDeTrabajo>>,
    /// URL pública del sitio, para los enlaces de los correos.
    pub url_publica: String,
}

#[derive(Debug, Clone)]
pub struct ModuloIdentidad {
    casos: Arc<CasosDeUso<UnidadDeTrabajo>>,
    limites: Arc<Limites>,
}

impl ModuloIdentidad {
    pub fn con_postgres(dependencias: DependenciasIdentidad) -> Self {
        let puertos = Puertos {
            transacciones: Arc::new(TransaccionesPg::nuevas(dependencias.pool)),
            usuarios: Arc::new(RepositorioUsuariosPg),
            sesiones: Arc::new(RepositorioSesionesPg),
            tokens: Arc::new(RepositorioTokensPg),
            perfiles: dependencias.perfiles,
            auditor: dependencias.auditor,
            buzon: dependencias.buzon,
            credenciales: Arc::new(CredencialesArgon2),
            reloj: dependencias.reloj,
            url_publica: dependencias.url_publica,
        };
        Self {
            casos: Arc::new(CasosDeUso::nuevos(puertos)),
            limites: Arc::new(Limites::estandar()),
        }
    }

    /// Rutas HTTP del módulo, para montarlas bajo `/api/v1`.
    pub fn rutas(&self) -> Router {
        http::rutas(EstadoHttp {
            casos: self.casos.clone(),
            limites: self.limites.clone(),
        })
    }

    /// Fachada de cuentas del personal para el módulo `barberos`.
    pub fn cuentas(&self) -> Arc<dyn CuentasPersonal<UnidadDeTrabajo>> {
        self.casos.cuentas.clone()
    }

    /// Validador de la cookie de sesión para toda la API.
    pub fn validador_sesiones(&self) -> Arc<dyn ValidadorSesion> {
        Arc::new(ValidadorIdentidad(self.casos.validar_sesion.clone()))
    }

    /// Invitación de un administrador. La usa la línea de comandos (`diamante
    /// crear-administrador`) con el actor `Sistema` para el primer administrador.
    pub async fn invitar_administrador(
        &self,
        actor: &Actor,
        correo: &str,
        nombre: &str,
    ) -> Result<InvitacionEmitida, ErrorIdentidad> {
        self.casos
            .invitar_administrador
            .ejecutar(actor, correo, nombre)
            .await
    }
}
