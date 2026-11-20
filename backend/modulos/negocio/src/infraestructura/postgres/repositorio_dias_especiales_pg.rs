use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdDiaEspecial;
use plataforma::base_datos::a_error_persistencia;
use sqlx::PgPool;

use crate::aplicacion::puertos::{DiaEspecialGuardado, RepositorioDiasEspeciales};
use crate::dominio::entidades::{DiaEspecial, OrigenDiaEspecial, TipoDiaEspecial};

#[derive(Debug)]
pub(crate) struct RepositorioDiasEspecialesPg {
    pool: PgPool,
}

impl RepositorioDiasEspecialesPg {
    pub(crate) fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

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
impl RepositorioDiasEspeciales for RepositorioDiasEspecialesPg {
    async fn listar(
        &self,
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
        .fetch_all(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        filas.into_iter().map(DiaEspecialGuardado::try_from).collect()
    }

    async fn registrar(&self, dia: &DiaEspecial) -> Result<IdDiaEspecial, ErrorPersistencia> {
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
        .fetch_one(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdDiaEspecial(id))
    }

    async fn eliminar(&self, id: IdDiaEspecial) -> Result<bool, ErrorPersistencia> {
        let resultado = sqlx::query("DELETE FROM negocio.dia_especial WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(a_error_persistencia)?;
        Ok(resultado.rows_affected() > 0)
    }
}
