//! Implementación del puerto `catalogo::api::OferentesServicio` (inversión de dependencias:
//! `barberos` depende de `catalogo`, nunca al revés).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use catalogo::api::OferentesServicio;
use nucleo::identificadores::IdServicio;
use nucleo::{ErrorPersistencia, Reloj};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

#[derive(Debug)]
pub(crate) struct OferentesServicioPg {
    reloj: Arc<dyn Reloj>,
}

impl OferentesServicioPg {
    pub(crate) fn nuevo(reloj: Arc<dyn Reloj>) -> Self {
        Self { reloj }
    }
}

#[async_trait]
impl OferentesServicio<UnidadDeTrabajo> for OferentesServicioPg {
    /// Barberos que lo ofrecen y no están retirados hoy.
    async fn contar(&self, uow: &mut UnidadDeTrabajo) -> Result<HashMap<IdServicio, i64>, ErrorPersistencia> {
        let filas: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT so.servicio_id, count(*)
             FROM barberos.servicio_ofrecido so
             JOIN barberos.barbero b ON b.id = so.barbero_id
             WHERE b.fecha_retiro IS NULL OR b.fecha_retiro > $1
             GROUP BY so.servicio_id",
        )
        .bind(self.reloj.hoy())
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(|(id, n)| (IdServicio(id), n)).collect())
    }
}
