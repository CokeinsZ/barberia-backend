use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdDiaEspecial;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{DiaEspecialGuardado, RepositorioDiasEspeciales};
use crate::dominio::entidades::{DiaEspecial, OrigenDiaEspecial, TipoDiaEspecial};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioDiasEspecialesPg;

#[derive(sqlx::FromRow)]
struct FilaDiaEspecial {
    id: i64,
    fecha: NaiveDate,
    nombre: String,
    tipo: String,
    hora_cierre: Option<NaiveTime>,
    origen: String,
}

impl TryFrom<FilaDiaEspecial> for DiaEspecialGuardado {
    type Error = ErrorPersistencia;

    fn try_from(fila: FilaDiaEspecial) -> Result<Self, Self::Error> {
        let tipo = TipoDiaEspecial::desde_codigo(&fila.tipo, fila.hora_cierre)
            .map_err(|e| ErrorPersistencia::Otro(format!("día especial {} inválido: {e:?}", fila.id)))?;
        let origen = match fila.origen.as_str() {
            "automatico" => OrigenDiaEspecial::Automatico,
            _ => OrigenDiaEspecial::Manual,
        };
        Ok(DiaEspecialGuardado {
            id: IdDiaEspecial(fila.id),
            dia: DiaEspecial::reconstruir(fila.fecha, fila.nombre, tipo, origen),
        })
    }
}

#[async_trait]
impl RepositorioDiasEspeciales<UnidadDeTrabajo> for RepositorioDiasEspecialesPg {
    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialGuardado>, ErrorPersistencia> {
        let filas: Vec<FilaDiaEspecial> = sqlx::query_as(
            "SELECT id, fecha, nombre, tipo::text AS tipo, hora_cierre, origen::text AS origen
             FROM negocio.dia_especial
             WHERE fecha BETWEEN $1 AND $2
             ORDER BY fecha",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        filas.into_iter().map(DiaEspecialGuardado::try_from).collect()
    }

    async fn registrar(
        &self,
        uow: &mut UnidadDeTrabajo,
        dia: &DiaEspecial,
    ) -> Result<IdDiaEspecial, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO negocio.dia_especial (fecha, nombre, tipo, hora_cierre, origen)
             VALUES ($1, $2, $3::negocio.tipo_dia_especial, $4, $5::negocio.origen_dia_especial)
             RETURNING id",
        )
        .bind(dia.fecha())
        .bind(dia.nombre())
        .bind(dia.tipo().codigo())
        .bind(dia.tipo().hora_cierre())
        .bind(dia.origen().codigo())
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdDiaEspecial(id))
    }

    async fn eliminar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia> {
        let fila: Option<FilaDiaEspecial> = sqlx::query_as(
            "DELETE FROM negocio.dia_especial WHERE id = $1
             RETURNING id, fecha, nombre, tipo::text AS tipo, hora_cierre, origen::text AS origen",
        )
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        fila.map(DiaEspecialGuardado::try_from).transpose()
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia> {
        let fila: Option<FilaDiaEspecial> = sqlx::query_as(
            "SELECT id, fecha, nombre, tipo::text AS tipo, hora_cierre, origen::text AS origen
             FROM negocio.dia_especial WHERE id = $1 FOR UPDATE",
        )
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        fila.map(DiaEspecialGuardado::try_from).transpose()
    }

    async fn actualizar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdDiaEspecial,
        dia: &DiaEspecial,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE negocio.dia_especial
             SET fecha = $2, nombre = $3, tipo = $4::negocio.tipo_dia_especial, hora_cierre = $5,
                 origen = $6::negocio.origen_dia_especial
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(dia.fecha())
        .bind(dia.nombre())
        .bind(dia.tipo().codigo())
        .bind(dia.tipo().hora_cierre())
        .bind(dia.origen().codigo())
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn registrar_si_libre(
        &self,
        uow: &mut UnidadDeTrabajo,
        dia: &DiaEspecial,
    ) -> Result<Option<IdDiaEspecial>, ErrorPersistencia> {
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO negocio.dia_especial (fecha, nombre, tipo, hora_cierre, origen)
             VALUES ($1, $2, $3::negocio.tipo_dia_especial, $4, $5::negocio.origen_dia_especial)
             ON CONFLICT (fecha) DO NOTHING
             RETURNING id",
        )
        .bind(dia.fecha())
        .bind(dia.nombre())
        .bind(dia.tipo().codigo())
        .bind(dia.tipo().hora_cierre())
        .bind(dia.origen().codigo())
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(id.map(IdDiaEspecial))
    }
}
