//! Cola `notificaciones.mensaje` en PostgreSQL.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::{ErrorPersistencia, Reloj};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::api::{BuzonSalida, CorreoSaliente};
use crate::aplicacion::puertos::{CorreoPendiente, RepositorioCorreos};

/// Encola dentro de la unidad de trabajo de quien llama (patrón outbox).
#[derive(Debug)]
pub(crate) struct BuzonPg {
    reloj: Arc<dyn Reloj>,
}

impl BuzonPg {
    pub(crate) fn nuevo(reloj: Arc<dyn Reloj>) -> Self {
        Self { reloj }
    }
}

#[async_trait]
impl BuzonSalida<UnidadDeTrabajo> for BuzonPg {
    async fn encolar_correo(
        &self,
        uow: &mut UnidadDeTrabajo,
        correo: CorreoSaliente,
    ) -> Result<(), ErrorPersistencia> {
        let contiene_secreto = correo.plantilla.contiene_secreto();
        // `{"plantilla": "...", "variables": {...}}`, por el formato serde de la enumeración.
        let mut valor = serde_json::to_value(&correo.plantilla)
            .map_err(|e| ErrorPersistencia::Otro(format!("plantilla no serializable: {e}")))?;
        let plantilla = valor["plantilla"].as_str().unwrap_or_default().to_owned();
        let variables = valor["variables"].take();
        let ahora = self.reloj.ahora();
        sqlx::query(
            "INSERT INTO notificaciones.mensaje
                 (canal, plantilla, destinatario, variables, contiene_secreto, programado_para, creado_en)
             VALUES ('correo', $1, $2, $3, $4, $5, $5)",
        )
        .bind(plantilla)
        .bind(correo.destinatario.como_str())
        .bind(variables)
        .bind(contiene_secreto)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}

/// Lectura y cambios de estado de la cola. Cada operación es atómica por sí sola: el envío
/// ocurre fuera de cualquier transacción.
#[derive(Debug)]
pub(crate) struct RepositorioCorreosPg {
    pool: PgPool,
}

impl RepositorioCorreosPg {
    pub(crate) fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct FilaPendiente {
    id: i64,
    destinatario: String,
    plantilla: String,
    variables: Value,
    intentos: i16,
}

#[async_trait]
impl RepositorioCorreos for RepositorioCorreosPg {
    async fn reservar(
        &self,
        ahora: DateTime<Utc>,
        reservar_hasta: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<CorreoPendiente>, ErrorPersistencia> {
        // SKIP LOCKED: dos workers nunca toman el mismo mensaje. Mover `programado_para` es la
        // reserva: si el worker muere sin marcar el resultado, el mensaje vuelve a estar
        // disponible cuando vence.
        let filas: Vec<FilaPendiente> = sqlx::query_as(
            "WITH lote AS (
                 SELECT id FROM notificaciones.mensaje
                 WHERE estado = 'pendiente' AND canal = 'correo' AND programado_para <= $1
                 ORDER BY programado_para
                 LIMIT $3
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE notificaciones.mensaje m
             SET programado_para = $2, intentos = m.intentos + 1
             FROM lote
             WHERE m.id = lote.id
             RETURNING m.id, m.destinatario, m.plantilla, m.variables, m.intentos",
        )
        .bind(ahora)
        .bind(reservar_hasta)
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(a_error_persistencia)?;

        Ok(filas
            .into_iter()
            .map(|f| CorreoPendiente {
                id: f.id,
                destinatario: f.destinatario,
                plantilla: serde_json::from_value(
                    json!({ "plantilla": f.plantilla, "variables": f.variables }),
                )
                .ok(),
                intentos: f.intentos,
            })
            .collect())
    }

    async fn marcar_enviado(&self, id: i64, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE notificaciones.mensaje
             SET estado = 'enviado', enviado_en = $2, ultimo_error = NULL,
                 variables = CASE WHEN contiene_secreto THEN '{}'::jsonb ELSE variables END
             WHERE id = $1",
        )
        .bind(id)
        .bind(ahora)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn marcar_fallo(
        &self,
        id: i64,
        error: &str,
        reintento: Option<DateTime<Utc>>,
    ) -> Result<(), ErrorPersistencia> {
        let error: String = error.chars().take(500).collect();
        match reintento {
            Some(cuando) => sqlx::query(
                "UPDATE notificaciones.mensaje SET ultimo_error = $2, programado_para = $3 WHERE id = $1",
            )
            .bind(id)
            .bind(error)
            .bind(cuando),
            // Un enlace con token que no se pudo entregar tampoco debe quedarse guardado.
            None => sqlx::query(
                "UPDATE notificaciones.mensaje
                 SET estado = 'fallido', ultimo_error = $2,
                     variables = CASE WHEN contiene_secreto THEN '{}'::jsonb ELSE variables END
                 WHERE id = $1",
            )
            .bind(id)
            .bind(error),
        }
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}
