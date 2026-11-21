//! Proyección de métricas: reacciones a eventos de otros módulos, en su misma transacción.

use async_trait::async_trait;
use contratos::agenda::InasistenciaMarcada;
use contratos::ventas::{VentaAnulada, VentaRegistrada};
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

/// `VentaRegistrada` y `VentaAnulada` (ventas): visitas, total gastado, última visita y
/// barbero habitual (RN-16, RF-CLI-03). Las ventas a «cliente ocasional» no suman.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProyeccionVentas;

#[async_trait]
impl ManejadorEvento<VentaRegistrada, UnidadDeTrabajo> for ProyeccionVentas {
    fn nombre(&self) -> &'static str {
        "clientes.proyeccion_ventas"
    }

    async fn manejar(
        &self,
        uow: &mut UnidadDeTrabajo,
        evento: &VentaRegistrada,
    ) -> Result<(), ErrorDeManejador> {
        let Some(cliente) = evento.cliente else {
            return Ok(());
        };
        sqlx::query(
            "INSERT INTO clientes.metricas (cliente_id, visitas, total_gastado, ultima_visita) VALUES ($1, 1, $2, $3)
             ON CONFLICT (cliente_id) DO UPDATE
             SET visitas = clientes.metricas.visitas + 1,
                 total_gastado = clientes.metricas.total_gastado + EXCLUDED.total_gastado,
                 ultima_visita = greatest(clientes.metricas.ultima_visita, EXCLUDED.ultima_visita)",
        )
        .bind(cliente.0)
        .bind(evento.venta_neta.valor())
        .bind(evento.fecha_operacion)
        .execute(uow.conexion())
        .await
        .map_err(ErrorDeManejador::nuevo)?;
        sqlx::query(
            "INSERT INTO clientes.visitas_por_barbero (cliente_id, barbero_id, visitas) VALUES ($1, $2, 1)
             ON CONFLICT (cliente_id, barbero_id) DO UPDATE SET visitas = clientes.visitas_por_barbero.visitas + 1",
        )
        .bind(cliente.0)
        .bind(evento.barbero.0)
        .execute(uow.conexion())
        .await
        .map_err(ErrorDeManejador::nuevo)?;
        Ok(())
    }
}

#[async_trait]
impl ManejadorEvento<VentaAnulada, UnidadDeTrabajo> for ProyeccionVentas {
    fn nombre(&self) -> &'static str {
        "clientes.proyeccion_ventas_anuladas"
    }

    async fn manejar(
        &self,
        uow: &mut UnidadDeTrabajo,
        evento: &VentaAnulada,
    ) -> Result<(), ErrorDeManejador> {
        let Some(cliente) = evento.cliente else {
            return Ok(());
        };
        sqlx::query(
            "UPDATE clientes.metricas
             SET visitas = greatest(visitas - 1, 0),
                 total_gastado = greatest(total_gastado - $2, 0),
                 ultima_visita = $3
             WHERE cliente_id = $1",
        )
        .bind(cliente.0)
        .bind(evento.venta_neta.valor())
        .bind(evento.ultima_visita_vigente)
        .execute(uow.conexion())
        .await
        .map_err(ErrorDeManejador::nuevo)?;
        sqlx::query(
            "WITH menos AS (
                 UPDATE clientes.visitas_por_barbero SET visitas = visitas - 1
                 WHERE cliente_id = $1 AND barbero_id = $2 AND visitas > 0
                 RETURNING cliente_id, barbero_id, visitas
             )
             DELETE FROM clientes.visitas_por_barbero v USING menos
             WHERE v.cliente_id = menos.cliente_id AND v.barbero_id = menos.barbero_id AND menos.visitas = 0",
        )
        .bind(cliente.0)
        .bind(evento.barbero.0)
        .execute(uow.conexion())
        .await
        .map_err(ErrorDeManejador::nuevo)?;
        Ok(())
    }
}
