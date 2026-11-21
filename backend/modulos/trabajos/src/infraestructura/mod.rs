//! Cola `trabajos.trabajo` en PostgreSQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use plataforma::base_datos::a_error_persistencia;
use serde_json::Value;
use sqlx::PgPool;

use crate::aplicacion::{RepositorioTrabajos, TrabajoTomado};

#[derive(Debug)]
pub(crate) struct RepositorioTrabajosPg {
    pool: PgPool,
}

impl RepositorioTrabajosPg {
    pub(crate) fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    tipo: String,
    carga: Value,
    ejecutar_en: DateTime<Utc>,
    intentos: i16,
    max_intentos: i16,
}

fn recortar(error: &str) -> String {
    error.chars().take(500).collect()
}

#[async_trait]
impl RepositorioTrabajos for RepositorioTrabajosPg {
    async fn programar(
        &self,
        tipo: &str,
        ejecutar_en: DateTime<Utc>,
        clave: &str,
    ) -> Result<bool, ErrorPersistencia> {
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO trabajos.trabajo (tipo, ejecutar_en, clave_unica, creado_en)
             VALUES ($1, $2, $3, $2)
             ON CONFLICT (clave_unica) DO NOTHING
             RETURNING id",
        )
        .bind(tipo)
        .bind(ejecutar_en)
        .bind(clave)
        .fetch_optional(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(id.is_some())
    }

    async fn tomar(
        &self,
        ahora: DateTime<Utc>,
        proceso: &str,
    ) -> Result<Option<TrabajoTomado>, ErrorPersistencia> {
        let fila: Option<Fila> = sqlx::query_as(
            "UPDATE trabajos.trabajo t
             SET estado = 'en_proceso', tomado_en = $1, tomado_por = $2, intentos = t.intentos + 1
             WHERE t.id = (
                 SELECT id FROM trabajos.trabajo
                 WHERE estado = 'pendiente' AND ejecutar_en <= $1
                 ORDER BY ejecutar_en, id
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING t.id, t.tipo, t.carga, t.ejecutar_en, t.intentos, t.max_intentos",
        )
        .bind(ahora)
        .bind(proceso)
        .fetch_optional(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|f| TrabajoTomado {
            id: f.id,
            tipo: f.tipo,
            carga: f.carga,
            ejecutar_en: f.ejecutar_en,
            intentos: f.intentos,
            max_intentos: f.max_intentos,
        }))
    }

    async fn completar(&self, id: i64, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE trabajos.trabajo SET estado = 'completado', terminado_en = $2, ultimo_error = NULL WHERE id = $1",
        )
        .bind(id)
        .bind(ahora)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn reintentar(&self, id: i64, error: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE trabajos.trabajo
             SET estado = 'pendiente', ejecutar_en = $3, ultimo_error = $2, tomado_en = NULL, tomado_por = NULL
             WHERE id = $1",
        )
        .bind(id)
        .bind(recortar(error))
        .bind(cuando)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn fallar(&self, id: i64, error: &str, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE trabajos.trabajo SET estado = 'fallido', terminado_en = $3, ultimo_error = $2 WHERE id = $1")
            .bind(id)
            .bind(recortar(error))
            .bind(ahora)
            .execute(&self.pool)
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn recuperar_abandonados(&self, limite: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
        sqlx::query(
            "UPDATE trabajos.trabajo
             SET estado = 'pendiente', tomado_en = NULL, tomado_por = NULL,
                 ultimo_error = 'abandonado: el proceso que lo tomó no terminó'
             WHERE estado = 'en_proceso' AND tomado_en < $1",
        )
        .bind(limite)
        .execute(&self.pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(a_error_persistencia)
    }
}

/// Mantenimiento: trabajos completados hace más de 30 días.
pub(crate) async fn limpiar(pool: &PgPool, antes_de: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
    sqlx::query("DELETE FROM trabajos.trabajo WHERE estado = 'completado' AND terminado_en < $1")
        .bind(antes_de)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(a_error_persistencia)
}
