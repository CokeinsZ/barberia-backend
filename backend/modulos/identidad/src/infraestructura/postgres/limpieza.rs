//! Mantenimiento nocturno: sesiones y enlaces que ya no sirven.

use chrono::{DateTime, TimeDelta, Utc};
use nucleo::ErrorPersistencia;
use plataforma::base_datos::a_error_persistencia;
use sqlx::PgPool;

/// Borra las sesiones vencidas o revocadas y los tokens vencidos o usados hace más de 30 días
/// (se conservan un tiempo para revisar accesos recientes).
pub(crate) async fn limpiar(pool: &PgPool, ahora: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
    let limite = ahora - TimeDelta::days(30);
    let sesiones = sqlx::query("DELETE FROM identidad.sesion WHERE expira_en < $1 OR revocada_en < $1")
        .bind(limite)
        .execute(pool)
        .await
        .map_err(a_error_persistencia)?
        .rows_affected();
    let tokens = sqlx::query("DELETE FROM identidad.token_usuario WHERE expira_en < $1 OR usado_en < $1")
        .bind(limite)
        .execute(pool)
        .await
        .map_err(a_error_persistencia)?
        .rows_affected();
    Ok(sesiones + tokens)
}
