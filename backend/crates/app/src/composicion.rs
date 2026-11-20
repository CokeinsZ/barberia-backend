//! Raíz de composición: construye los adaptadores y los inyecta en los módulos. Es el único
//! lugar que conoce a todos; los módulos nunca crean sus dependencias externas.

use std::sync::Arc;

use auditoria::ModuloAuditoria;
use barberos::ModuloBarberos;
use identidad::{DependenciasIdentidad, ModuloIdentidad};
use negocio::ModuloNegocio;
use notificaciones::ModuloNotificaciones;
use notificaciones::api::ErrorNotificaciones;
use nucleo::{Reloj, RelojSistema};
use plataforma::base_datos::conectar;
use plataforma::configuracion::Configuracion;
use plataforma::http::{ConfianzaProxy, OrigenesPermitidos, Sesiones};
use sqlx::PgPool;

use crate::ErrorDeArranque;

#[derive(Debug, Clone)]
pub(crate) struct Composicion {
    pub(crate) pool: PgPool,
    pub(crate) sesiones: Sesiones,
    pub(crate) origenes: OrigenesPermitidos,
    pub(crate) confianza_proxy: ConfianzaProxy,
    pub(crate) identidad: ModuloIdentidad,
    pub(crate) negocio: ModuloNegocio,
    pub(crate) auditoria: ModuloAuditoria,
    pub(crate) notificaciones: ModuloNotificaciones,
}

impl Composicion {
    pub(crate) async fn construir(configuracion: &Configuracion) -> Result<Self, ErrorDeArranque> {
        let pool = conectar(
            &configuracion.base_datos.url,
            configuracion.base_datos.max_conexiones,
        )
        .await?;
        Ok(Self::con_pool(pool, Arc::new(RelojSistema), configuracion)?)
    }

    /// Orden de construcción: primero los módulos que aportan puertos (auditoría, buzón de
    /// salida, perfiles de barberos) y después los que los usan.
    pub(crate) fn con_pool(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        configuracion: &Configuracion,
    ) -> Result<Self, ErrorNotificaciones> {
        let auditoria = ModuloAuditoria::con_postgres(pool.clone(), reloj.clone());
        let notificaciones =
            ModuloNotificaciones::con_postgres(pool.clone(), reloj.clone(), configuracion.smtp.as_ref())?;
        let barberos = ModuloBarberos::con_postgres(reloj.clone());
        let identidad = ModuloIdentidad::con_postgres(DependenciasIdentidad {
            pool: pool.clone(),
            reloj: reloj.clone(),
            perfiles: barberos.perfiles_para_identidad(),
            auditor: auditoria.auditor(),
            buzon: notificaciones.buzon(),
            url_publica: configuracion.url_publica.clone(),
        });
        let negocio = ModuloNegocio::con_postgres(pool.clone(), reloj, auditoria.auditor());

        Ok(Self {
            pool,
            sesiones: Sesiones(identidad.validador_sesiones()),
            origenes: OrigenesPermitidos(Arc::new(configuracion.origenes_permitidos.clone())),
            confianza_proxy: ConfianzaProxy(configuracion.confiar_proxy),
            identidad,
            negocio,
            auditoria,
            notificaciones,
        })
    }
}
