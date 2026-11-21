use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{PoliticaGuardada, RepositorioPoliticas};
use crate::dominio::entidades::{NuevaPolitica, TipoPolitica};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioPoliticasPg;

#[derive(sqlx::FromRow)]
struct Fila {
    id: i16,
    tipo: String,
    version: String,
    url: String,
    vigente_desde: DateTime<Utc>,
}

#[async_trait]
impl RepositorioPoliticas<UnidadDeTrabajo> for RepositorioPoliticasPg {
    async fn listar(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<PoliticaGuardada>, ErrorPersistencia> {
        let filas: Vec<Fila> = sqlx::query_as(
            "SELECT id, tipo::text AS tipo, version, url, vigente_desde
             FROM negocio.politica_version ORDER BY vigente_desde DESC, id DESC",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        filas
            .into_iter()
            .map(|f| {
                Ok(PoliticaGuardada {
                    id: i64::from(f.id),
                    tipo: TipoPolitica::desde_codigo(&f.tipo).ok_or_else(|| {
                        ErrorPersistencia::Otro(format!("tipo de política desconocido: {}", f.tipo))
                    })?,
                    version: f.version,
                    url: f.url,
                    vigente_desde: f.vigente_desde,
                })
            })
            .collect()
    }

    async fn publicar(&self, uow: &mut UnidadDeTrabajo, p: &NuevaPolitica) -> Result<i64, ErrorPersistencia> {
        let id: i16 = sqlx::query_scalar(
            "INSERT INTO negocio.politica_version (tipo, version, vigente_desde, url)
             VALUES ($1::negocio.tipo_politica, $2, $3, $4)
             RETURNING id",
        )
        .bind(p.tipo.codigo())
        .bind(&p.version)
        .bind(p.vigente_desde)
        .bind(&p.url)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(i64::from(id))
    }
}
