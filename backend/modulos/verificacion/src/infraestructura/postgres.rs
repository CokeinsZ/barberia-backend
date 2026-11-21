use std::net::IpAddr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::{Celular, ErrorPersistencia};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use plataforma::seguridad::Huella;

use crate::aplicacion::puertos::{CodigoGuardado, RepositorioCodigos};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioCodigosPg;

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    celular: String,
    codigo_hash: Vec<u8>,
    intentos: i16,
    max_intentos: i16,
    expira_en: DateTime<Utc>,
    verificado: bool,
}

#[async_trait]
impl RepositorioCodigos<UnidadDeTrabajo> for RepositorioCodigosPg {
    async fn ultimo(
        &self,
        uow: &mut UnidadDeTrabajo,
        celular: &Celular,
    ) -> Result<Option<DateTime<Utc>>, ErrorPersistencia> {
        sqlx::query_scalar("SELECT max(creado_en) FROM verificacion.codigo WHERE celular = $1")
            .bind(celular.digitos())
            .fetch_one(uow.conexion())
            .await
            .map_err(a_error_persistencia)
    }

    async fn crear(
        &self,
        uow: &mut UnidadDeTrabajo,
        celular: &Celular,
        huella: &[u8; 32],
        expira_en: DateTime<Utc>,
        ip: Option<IpAddr>,
        ahora: DateTime<Utc>,
    ) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar(
            "INSERT INTO verificacion.codigo (celular, codigo_hash, creado_en, expira_en, ip)
             VALUES ($1, $2, $3, $4, $5::inet) RETURNING id",
        )
        .bind(celular.digitos())
        .bind(&huella[..])
        .bind(ahora)
        .bind(expira_en)
        .bind(ip.map(|i| i.to_string()))
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn para_confirmar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
    ) -> Result<Option<CodigoGuardado>, ErrorPersistencia> {
        let fila: Option<Fila> = sqlx::query_as(
            "SELECT id, celular::text AS celular, codigo_hash, intentos, max_intentos, expira_en,
                    verificado_en IS NOT NULL AS verificado
             FROM verificacion.codigo WHERE id = $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|f| CodigoGuardado {
            id: f.id,
            celular: f.celular,
            huella: f.codigo_hash,
            intentos: f.intentos,
            max_intentos: f.max_intentos,
            expira_en: f.expira_en,
            verificado: f.verificado,
        }))
    }

    async fn registrar_fallo(&self, uow: &mut UnidadDeTrabajo, id: i64) -> Result<i16, ErrorPersistencia> {
        sqlx::query_scalar(
            "UPDATE verificacion.codigo SET intentos = intentos + 1 WHERE id = $1 RETURNING intentos",
        )
        .bind(id)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn marcar_verificado(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
        comprobante: &Huella,
        comprobante_expira_en: DateTime<Utc>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE verificacion.codigo
             SET verificado_en = $2, comprobante_hash = $3, comprobante_expira_en = $4
             WHERE id = $1",
        )
        .bind(id)
        .bind(ahora)
        .bind(&comprobante[..])
        .bind(comprobante_expira_en)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn consumir_comprobante(
        &self,
        uow: &mut UnidadDeTrabajo,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<String>, ErrorPersistencia> {
        sqlx::query_scalar(
            "UPDATE verificacion.codigo SET comprobante_usado_en = $2
             WHERE comprobante_hash = $1 AND comprobante_usado_en IS NULL AND comprobante_expira_en > $2
             RETURNING celular::text",
        )
        .bind(&huella[..])
        .bind(ahora)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }
}

/// Mantenimiento nocturno: códigos vencidos hace más de 7 días (ya no sirven ni para revisar abusos).
pub(crate) async fn limpiar(
    pool: &sqlx::PgPool,
    ahora: chrono::DateTime<chrono::Utc>,
) -> Result<u64, ErrorPersistencia> {
    sqlx::query("DELETE FROM verificacion.codigo WHERE expira_en < $1 AND (comprobante_expira_en IS NULL OR comprobante_expira_en < $1)")
        .bind(ahora - chrono::TimeDelta::days(7))
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(a_error_persistencia)
}
