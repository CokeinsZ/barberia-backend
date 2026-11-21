//! Fachada de solo lectura para `ventas`: quién puede registrar ventas y con qué comisión.

use async_trait::async_trait;
use chrono::NaiveDate;
use nucleo::identificadores::{IdBarbero, IdServicio};
use nucleo::{ErrorPersistencia, Porcentaje};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::api::{BarberoVenta, EquipoVentas};

#[derive(Debug, Default)]
pub(crate) struct EquipoVentasPg;

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    nombre: String,
    comision_centesimas: i32,
    activo: bool,
    servicios: Vec<i64>,
}

#[async_trait]
impl EquipoVentas<UnidadDeTrabajo> for EquipoVentasPg {
    async fn para_venta(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        fecha: NaiveDate,
    ) -> Result<Option<BarberoVenta>, ErrorPersistencia> {
        // Activo en la fecha: sin retiro y sin ausencia vigente (RN-24).
        let fila: Option<Fila> = sqlx::query_as(
            "SELECT b.id, b.nombres || ' ' || b.apellidos AS nombre,
                    (b.porcentaje_comision * 100)::int4 AS comision_centesimas,
                    (b.fecha_retiro IS NULL OR b.fecha_retiro > $2)
                      AND NOT EXISTS (
                          SELECT 1 FROM barberos.ausencia a
                          WHERE a.barbero_id = b.id AND a.anulada_en IS NULL AND $2 BETWEEN a.desde AND a.hasta
                      ) AS activo,
                    coalesce((SELECT array_agg(s.servicio_id ORDER BY s.servicio_id)
                              FROM barberos.servicio_ofrecido s WHERE s.barbero_id = b.id), '{}') AS servicios
             FROM barberos.barbero b WHERE b.id = $1",
        )
        .bind(barbero.0)
        .bind(fecha)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        fila.map(|f| {
            let centesimas = u16::try_from(f.comision_centesimas)
                .map_err(|_| ErrorPersistencia::Otro("porcentaje de comisión fuera de rango".into()))?;
            Ok(BarberoVenta {
                id: IdBarbero(f.id),
                nombre: f.nombre,
                activo: f.activo,
                servicios: f.servicios.into_iter().map(IdServicio).collect(),
                porcentaje_comision: Porcentaje::desde_centesimas(centesimas)
                    .map_err(|e| ErrorPersistencia::Otro(e.to_string()))?,
            })
        })
        .transpose()
    }

    async fn nombres(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[IdBarbero],
    ) -> Result<std::collections::HashMap<IdBarbero, String>, ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        let filas: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, nombres || ' ' || apellidos FROM barberos.barbero WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(id, nombre)| (IdBarbero(id), nombre))
            .collect())
    }
}
