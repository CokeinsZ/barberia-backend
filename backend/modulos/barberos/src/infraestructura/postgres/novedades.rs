use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::{IdArchivo, IdAusencia, IdBarbero, IdBloqueo, IdUsuario};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{AusenciaGuardada, BloqueoGuardado, RepositorioNovedades};
use crate::dominio::{NuevaAusencia, NuevoBloqueo};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioNovedadesPg;

const COLUMNAS_AUSENCIA: &str = "id, tipo::text AS tipo, desde, hasta, observacion, soporte_archivo_id, registrada_en, (anulada_en IS NOT NULL) AS anulada";

#[derive(sqlx::FromRow)]
struct FilaAusencia {
    id: i64,
    tipo: String,
    desde: NaiveDate,
    hasta: NaiveDate,
    observacion: Option<String>,
    soporte_archivo_id: Option<i64>,
    registrada_en: DateTime<Utc>,
    anulada: bool,
}

impl From<FilaAusencia> for AusenciaGuardada {
    fn from(f: FilaAusencia) -> Self {
        AusenciaGuardada {
            id: IdAusencia(f.id),
            tipo: f.tipo,
            desde: f.desde,
            hasta: f.hasta,
            observacion: f.observacion,
            soporte: f.soporte_archivo_id.map(IdArchivo),
            registrada_en: f.registrada_en,
            anulada: f.anulada,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FilaBloqueo {
    id: i64,
    inicio: DateTime<Utc>,
    fin: DateTime<Utc>,
    motivo: String,
    nota: Option<String>,
}

impl From<FilaBloqueo> for BloqueoGuardado {
    fn from(f: FilaBloqueo) -> Self {
        BloqueoGuardado {
            id: IdBloqueo(f.id),
            inicio: f.inicio,
            fin: f.fin,
            motivo: f.motivo,
            nota: f.nota,
        }
    }
}

#[async_trait]
impl RepositorioNovedades<UnidadDeTrabajo> for RepositorioNovedadesPg {
    async fn ausencias(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
    ) -> Result<Vec<AusenciaGuardada>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT {COLUMNAS_AUSENCIA} FROM barberos.ausencia WHERE barbero_id = $1 ORDER BY desde DESC, id DESC"
        );
        // La consulta se arma solo con constantes de este archivo.
        let filas: Vec<FilaAusencia> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(barbero.0)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(AusenciaGuardada::from).collect())
    }

    async fn registrar_ausencia(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        a: &NuevaAusencia,
        por: IdUsuario,
        ahora: DateTime<Utc>,
    ) -> Result<IdAusencia, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO barberos.ausencia (barbero_id, tipo, desde, hasta, observacion, registrada_por, registrada_en)
             VALUES ($1, $2::barberos.tipo_ausencia, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(barbero.0)
        .bind(a.tipo.codigo())
        .bind(a.desde)
        .bind(a.hasta)
        .bind(&a.observacion)
        .bind(por.0)
        .bind(ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdAusencia(id))
    }

    async fn anular_ausencia(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        id: IdAusencia,
        ahora: DateTime<Utc>,
    ) -> Result<Option<AusenciaGuardada>, ErrorPersistencia> {
        let consulta = format!(
            "UPDATE barberos.ausencia SET anulada_en = $3
             WHERE id = $2 AND barbero_id = $1 AND anulada_en IS NULL
             RETURNING {COLUMNAS_AUSENCIA}"
        );
        let fila: Option<FilaAusencia> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(barbero.0)
            .bind(id.0)
            .bind(ahora)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(fila.map(AusenciaGuardada::from))
    }

    async fn poner_soporte(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        id: IdAusencia,
        archivo: IdArchivo,
    ) -> Result<Option<AusenciaGuardada>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT {COLUMNAS_AUSENCIA} FROM barberos.ausencia WHERE id = $2 AND barbero_id = $1 FOR UPDATE"
        );
        let anterior: Option<FilaAusencia> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(barbero.0)
            .bind(id.0)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        if anterior.is_some() {
            sqlx::query("UPDATE barberos.ausencia SET soporte_archivo_id = $2 WHERE id = $1")
                .bind(id.0)
                .bind(archivo.0)
                .execute(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        }
        Ok(anterior.map(AusenciaGuardada::from))
    }

    async fn bloqueos(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        desde: DateTime<Utc>,
    ) -> Result<Vec<BloqueoGuardado>, ErrorPersistencia> {
        let filas: Vec<FilaBloqueo> = sqlx::query_as(
            "SELECT id, inicio, fin, motivo::text AS motivo, nota FROM barberos.bloqueo
             WHERE barbero_id = $1 AND fin > $2 ORDER BY inicio",
        )
        .bind(barbero.0)
        .bind(desde)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(BloqueoGuardado::from).collect())
    }

    async fn crear_bloqueo(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        b: &NuevoBloqueo,
        por: IdUsuario,
        ahora: DateTime<Utc>,
    ) -> Result<IdBloqueo, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO barberos.bloqueo (barbero_id, inicio, fin, motivo, nota, creado_por, creado_en)
             VALUES ($1, $2, $3, $4::barberos.motivo_bloqueo, $5, $6, $7)
             RETURNING id",
        )
        .bind(barbero.0)
        .bind(b.inicio)
        .bind(b.fin)
        .bind(b.motivo.codigo())
        .bind(&b.nota)
        .bind(por.0)
        .bind(ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdBloqueo(id))
    }

    async fn eliminar_bloqueo(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        id: IdBloqueo,
    ) -> Result<Option<BloqueoGuardado>, ErrorPersistencia> {
        let fila: Option<FilaBloqueo> = sqlx::query_as(
            "DELETE FROM barberos.bloqueo WHERE id = $2 AND barbero_id = $1
             RETURNING id, inicio, fin, motivo::text AS motivo, nota",
        )
        .bind(barbero.0)
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(BloqueoGuardado::from))
    }
}
