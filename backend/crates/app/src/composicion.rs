//! Raíz de composición: construye los adaptadores y los inyecta en los módulos. Es el único
//! lugar que conoce a todos; los módulos nunca crean sus dependencias externas.

use std::sync::Arc;

use negocio::ModuloNegocio;
use nucleo::{Reloj, RelojSistema};
use plataforma::base_datos::conectar;
use plataforma::configuracion::Configuracion;
use plataforma::http::{Sesiones, SinSesiones};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub(crate) struct Composicion {
    pub(crate) pool: PgPool,
    pub(crate) sesiones: Sesiones,
    pub(crate) negocio: ModuloNegocio,
}

impl Composicion {
    pub(crate) async fn construir(configuracion: &Configuracion) -> Result<Self, sqlx::Error> {
        let pool = conectar(
            &configuracion.base_datos.url,
            configuracion.base_datos.max_conexiones,
        )
        .await?;
        Ok(Self::con_pool(pool, Arc::new(RelojSistema)))
    }

    pub(crate) fn con_pool(pool: PgPool, reloj: Arc<dyn Reloj>) -> Self {
        // Fase 1: el módulo `identidad` aporta el validador real. Mientras tanto ninguna
        // sesión es válida y todas las rutas protegidas responden 401 (seguro por defecto).
        let sesiones = Sesiones(Arc::new(SinSesiones));
        let negocio = ModuloNegocio::con_postgres(pool.clone(), reloj);
        Self {
            pool,
            sesiones,
            negocio,
        }
    }
}
