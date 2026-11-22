use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdUsuario;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use plataforma::seguridad::Huella;

use super::usuarios::{COLUMNAS_USUARIO, FilaUsuario};
use crate::aplicacion::puertos::{NuevaSesion, RepositorioSesiones, SesionVigente};
use crate::dominio::DuracionSesion;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioSesionesPg;

#[derive(sqlx::FromRow)]
struct FilaSesion {
    id: i64,
    recordarme: bool,
    creada_en: DateTime<Utc>,
    ultimo_uso_en: DateTime<Utc>,
    #[sqlx(flatten)]
    usuario: FilaUsuario,
}

#[async_trait]
impl RepositorioSesiones<UnidadDeTrabajo> for RepositorioSesionesPg {
    async fn crear(&self, uow: &mut UnidadDeTrabajo, sesion: &NuevaSesion) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO identidad.sesion
                 (usuario_id, token_hash, recordarme, creada_en, ultimo_uso_en, expira_en, ip, agente_usuario)
             VALUES ($1, $2, $3, $4, $4, $5, $6::inet, $7)",
        )
        .bind(sesion.usuario.0)
        .bind(&sesion.huella[..])
        .bind(sesion.duracion.recordarme())
        .bind(sesion.ahora)
        .bind(sesion.duracion.expira_en(sesion.ahora))
        .bind(sesion.ip.map(|ip| ip.to_string()))
        .bind(&sesion.agente)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn vigente(
        &self,
        uow: &mut UnidadDeTrabajo,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<SesionVigente>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT s.id, s.recordarme, s.creada_en, s.ultimo_uso_en, {COLUMNAS_USUARIO}
             FROM identidad.sesion s
             JOIN identidad.usuario u ON u.id = s.usuario_id
             LEFT JOIN identidad.administrador a ON a.usuario_id = u.id
             WHERE s.token_hash = $1 AND s.revocada_en IS NULL AND s.expira_en > $2"
        );
        // La consulta se arma solo con constantes del módulo.
        let fila: Option<FilaSesion> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(&huella[..])
            .bind(ahora)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        fila.map(|f| {
            Ok(SesionVigente {
                id: f.id,
                usuario: f.usuario.a_dominio()?,
                duracion: DuracionSesion::desde_recordarme(f.recordarme),
                creada_en: f.creada_en,
                ultimo_uso_en: f.ultimo_uso_en,
            })
        })
        .transpose()
    }

    async fn renovar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
        ahora: DateTime<Utc>,
        expira_en: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE identidad.sesion SET ultimo_uso_en = $2, expira_en = $3 WHERE id = $1")
            .bind(id)
            .bind(ahora)
            .bind(expira_en)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn revocar(
        &self,
        uow: &mut UnidadDeTrabajo,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE identidad.sesion SET revocada_en = $2 WHERE token_hash = $1 AND revocada_en IS NULL",
        )
        .bind(&huella[..])
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn revocar_de_usuario(
        &self,
        uow: &mut UnidadDeTrabajo,
        usuario: IdUsuario,
        excepto: Option<&Huella>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE identidad.sesion SET revocada_en = $3
             WHERE usuario_id = $1 AND revocada_en IS NULL AND ($2::bytea IS NULL OR token_hash <> $2)",
        )
        .bind(usuario.0)
        .bind(excepto.map(|h| &h[..]))
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}
