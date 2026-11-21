//! Proyección de métricas: reacciones a eventos de otros módulos, en su misma transacción.

use async_trait::async_trait;
use contratos::agenda::InasistenciaMarcada;
use nucleo::eventos::{ErrorDeManejador, ManejadorEvento};
use plataforma::base_datos::UnidadDeTrabajo;

/// `InasistenciaMarcada` (agenda) suma una inasistencia al cliente (RF-AGE-08, RN-10).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProyeccionInasistencias;

#[async_trait]
impl ManejadorEvento<InasistenciaMarcada, UnidadDeTrabajo> for ProyeccionInasistencias {
    fn nombre(&self) -> &'static str {
        "clientes.proyeccion_inasistencias"
    }

    async fn manejar(
        &self,
        uow: &mut UnidadDeTrabajo,
        evento: &InasistenciaMarcada,
    ) -> Result<(), ErrorDeManejador> {
        sqlx::query(
            "INSERT INTO clientes.metricas (cliente_id, inasistencias) VALUES ($1, 1)
             ON CONFLICT (cliente_id) DO UPDATE SET inasistencias = clientes.metricas.inasistencias + 1",
        )
        .bind(evento.cliente.0)
        .execute(uow.conexion())
        .await
        .map_err(ErrorDeManejador::nuevo)?;
        Ok(())
    }
}
