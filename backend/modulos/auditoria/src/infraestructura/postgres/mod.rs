//! Escritura y lectura de `auditoria.registro` con PostgreSQL.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::identificadores::IdUsuario;
use nucleo::{ErrorPersistencia, Reloj};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use serde_json::Value;

use crate::api::{Auditor, EntradaAuditoria};
use crate::aplicacion::dto::RegistroAuditoriaDto;
use crate::aplicacion::puertos::{ConsultaRegistros, RepositorioRegistros};

/// Implementación de [`Auditor`] para la unidad de trabajo de PostgreSQL.
#[derive(Debug)]
pub(crate) struct AuditorPg {
    reloj: Arc<dyn Reloj>,
}

impl AuditorPg {
    pub(crate) fn nuevo(reloj: Arc<dyn Reloj>) -> Self {
        Self { reloj }
    }
}

#[async_trait]
impl Auditor<UnidadDeTrabajo> for AuditorPg {
    async fn registrar(
        &self,
        uow: &mut UnidadDeTrabajo,
        entrada: EntradaAuditoria,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO auditoria.registro
                 (ocurrido_en, usuario_id, actor, modulo, accion, entidad, entidad_id, antes, despues)
             VALUES ($1, $2, $3::nucleo.actor, $4, $5, $6, $7, $8, $9)",
        )
        .bind(self.reloj.ahora())
        .bind(entrada.actor.usuario().map(|u| u.0))
        .bind(entrada.actor.tipo().como_str())
        .bind(entrada.modulo)
        .bind(entrada.accion)
        .bind(entrada.entidad)
        .bind(&entrada.entidad_id)
        .bind(&entrada.antes)
        .bind(&entrada.despues)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct RepositorioRegistrosPg;

#[derive(sqlx::FromRow)]
struct FilaRegistro {
    id: i64,
    ocurrido_en: DateTime<Utc>,
    actor: String,
    usuario_id: Option<i64>,
    modulo: String,
    accion: String,
    entidad: String,
    entidad_id: String,
    antes: Option<Value>,
    despues: Option<Value>,
    total: i64,
}

#[async_trait]
impl RepositorioRegistros<UnidadDeTrabajo> for RepositorioRegistrosPg {
    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        consulta: &ConsultaRegistros,
    ) -> Result<(Vec<RegistroAuditoriaDto>, i64), ErrorPersistencia> {
        let filas: Vec<FilaRegistro> = sqlx::query_as(
            "SELECT id, ocurrido_en, actor::text AS actor, usuario_id, modulo, accion, entidad, entidad_id,
                    antes, despues, count(*) OVER () AS total
             FROM auditoria.registro
             WHERE ($1::text IS NULL OR entidad = $1)
               AND ($2::text IS NULL OR entidad_id = $2)
               AND ($3::timestamptz IS NULL OR ocurrido_en >= $3)
               AND ($4::timestamptz IS NULL OR ocurrido_en < $4)
             ORDER BY ocurrido_en DESC, id DESC
             LIMIT $5 OFFSET $6",
        )
        .bind(&consulta.entidad)
        .bind(&consulta.entidad_id)
        .bind(consulta.desde)
        .bind(consulta.hasta)
        .bind(consulta.pagina.limite())
        .bind(consulta.pagina.desplazamiento())
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;

        let total = filas.first().map_or(0, |f| f.total);
        let registros = filas
            .into_iter()
            .map(|f| RegistroAuditoriaDto {
                id: f.id,
                ocurrido_en: f.ocurrido_en,
                actor: f.actor,
                usuario_id: f.usuario_id.map(IdUsuario),
                modulo: f.modulo,
                accion: f.accion,
                entidad: f.entidad,
                entidad_id: f.entidad_id,
                antes: f.antes,
                despues: f.despues,
            })
            .collect();
        Ok((registros, total))
    }
}
