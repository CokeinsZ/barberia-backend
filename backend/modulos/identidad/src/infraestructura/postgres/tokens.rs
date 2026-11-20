use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdUsuario;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use plataforma::seguridad::Huella;

use crate::aplicacion::puertos::RepositorioTokens;
use crate::dominio::PropositoToken;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioTokensPg;

#[async_trait]
impl RepositorioTokens<UnidadDeTrabajo> for RepositorioTokensPg {
    async fn emitir(
        &self,
        uow: &mut UnidadDeTrabajo,
        usuario: IdUsuario,
        proposito: PropositoToken,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        // Los enlaces anteriores quedan inservibles (se marcan como usados).
        sqlx::query(
            "UPDATE identidad.token_usuario SET usado_en = $3
             WHERE usuario_id = $1 AND proposito = $2::identidad.proposito_token AND usado_en IS NULL",
        )
        .bind(usuario.0)
        .bind(proposito.como_str())
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;

        sqlx::query(
            "INSERT INTO identidad.token_usuario (usuario_id, proposito, token_hash, creado_en, expira_en)
             VALUES ($1, $2::identidad.proposito_token, $3, $4, $5)",
        )
        .bind(usuario.0)
        .bind(proposito.como_str())
        .bind(&huella[..])
        .bind(ahora)
        .bind(ahora + proposito.vigencia())
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn consumir(
        &self,
        uow: &mut UnidadDeTrabajo,
        huella: &Huella,
        proposito: PropositoToken,
        ahora: DateTime<Utc>,
    ) -> Result<Option<IdUsuario>, ErrorPersistencia> {
        let usuario: Option<i64> = sqlx::query_scalar(
            "UPDATE identidad.token_usuario SET usado_en = $3
             WHERE token_hash = $1 AND proposito = $2::identidad.proposito_token
               AND usado_en IS NULL AND expira_en > $3
             RETURNING usuario_id",
        )
        .bind(&huella[..])
        .bind(proposito.como_str())
        .bind(ahora)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(usuario.map(IdUsuario))
    }
}
