//! Cola `notificaciones.mensaje`, webhooks y avisos en PostgreSQL.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use nucleo::{ErrorPersistencia, Pagina, Reloj, SolicitudPagina};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use crate::api::{
    Avisos, BuzonSalida, ConsultaMensajes, CorreoSaliente, EstadoMensaje, NuevoAviso, WhatsAppSaliente,
};
use crate::aplicacion::puertos::{
    Canal, EventoWebhook, MensajePendiente, MensajeRegistrado, RepositorioMensajes, RepositorioWebhooks,
};
use crate::dominio::webhook::EstadoEntrega;

/// Encola dentro de la unidad de trabajo de quien llama (patrón outbox), registra avisos y
/// consulta estados de mensajes.
#[derive(Debug)]
pub(crate) struct BuzonPg {
    reloj: Arc<dyn Reloj>,
}

impl BuzonPg {
    pub(crate) fn nuevo(reloj: Arc<dyn Reloj>) -> Self {
        Self { reloj }
    }

    async fn encolar(
        &self,
        conexion: &mut PgConnection,
        canal: Canal,
        destinatario: &str,
        plantilla: &impl Serialize,
        contiene_secreto: bool,
    ) -> Result<i64, ErrorPersistencia> {
        // `{"plantilla": "...", "variables": {...}}`, por el formato serde de las enumeraciones.
        let mut valor = serde_json::to_value(plantilla)
            .map_err(|e| ErrorPersistencia::Otro(format!("plantilla no serializable: {e}")))?;
        let nombre = valor["plantilla"].as_str().unwrap_or_default().to_owned();
        let variables = valor["variables"].take();
        let ahora = self.reloj.ahora();
        sqlx::query_scalar(
            "INSERT INTO notificaciones.mensaje
                 (canal, plantilla, destinatario, variables, contiene_secreto, programado_para, creado_en, actualizado_en)
             VALUES ($1::notificaciones.canal, $2, $3, $4, $5, $6, $6, $6)
             RETURNING id",
        )
        .bind(canal.codigo())
        .bind(nombre)
        .bind(destinatario)
        .bind(variables)
        .bind(contiene_secreto)
        .bind(ahora)
        .fetch_one(conexion)
        .await
        .map_err(a_error_persistencia)
    }
}

#[async_trait]
impl BuzonSalida<UnidadDeTrabajo> for BuzonPg {
    async fn encolar_correo(
        &self,
        uow: &mut UnidadDeTrabajo,
        correo: CorreoSaliente,
    ) -> Result<(), ErrorPersistencia> {
        self.encolar(
            uow.conexion(),
            Canal::Correo,
            correo.destinatario.como_str(),
            &correo.plantilla,
            correo.plantilla.contiene_secreto(),
        )
        .await
        .map(|_| ())
    }

    async fn encolar_whatsapp(
        &self,
        uow: &mut UnidadDeTrabajo,
        mensaje: WhatsAppSaliente,
    ) -> Result<i64, ErrorPersistencia> {
        self.encolar(
            uow.conexion(),
            Canal::WhatsApp,
            mensaje.destinatario.digitos(),
            &mensaje.plantilla,
            mensaje.plantilla.contiene_secreto(),
        )
        .await
    }
}

async fn insertar_aviso(
    conexion: &mut PgConnection,
    aviso: &NuevoAviso,
    ahora: DateTime<Utc>,
) -> Result<i64, ErrorPersistencia> {
    sqlx::query_scalar(
        "INSERT INTO notificaciones.aviso (tipo, titulo, detalle, datos, creado_en)
         VALUES ($1::notificaciones.tipo_aviso, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(aviso.tipo.codigo())
    .bind(&aviso.titulo)
    .bind(&aviso.detalle)
    .bind(&aviso.datos)
    .bind(ahora)
    .fetch_one(conexion)
    .await
    .map_err(a_error_persistencia)
}

#[async_trait]
impl Avisos<UnidadDeTrabajo> for BuzonPg {
    async fn crear(&self, uow: &mut UnidadDeTrabajo, aviso: NuevoAviso) -> Result<i64, ErrorPersistencia> {
        insertar_aviso(uow.conexion(), &aviso, self.reloj.ahora()).await
    }

    async fn no_leidos(&self, uow: &mut UnidadDeTrabajo) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar("SELECT count(*) FROM notificaciones.aviso WHERE leido_en IS NULL")
            .fetch_one(uow.conexion())
            .await
            .map_err(a_error_persistencia)
    }
}

#[derive(sqlx::FromRow)]
struct FilaEstado {
    id: i64,
    canal: String,
    plantilla: String,
    estado: String,
    intentos: i16,
    ultimo_error: Option<String>,
    creado_en: DateTime<Utc>,
    enviado_en: Option<DateTime<Utc>>,
}

impl From<FilaEstado> for EstadoMensaje {
    fn from(f: FilaEstado) -> Self {
        EstadoMensaje {
            id: f.id,
            canal: f.canal,
            plantilla: f.plantilla,
            estado: f.estado,
            intentos: f.intentos,
            ultimo_error: f.ultimo_error,
            creado_en: f.creado_en,
            enviado_en: f.enviado_en,
        }
    }
}

const COLUMNAS_ESTADO: &str = "id, canal::text AS canal, plantilla, estado::text AS estado, intentos, ultimo_error, \
                               creado_en, enviado_en";

#[async_trait]
impl ConsultaMensajes<UnidadDeTrabajo> for BuzonPg {
    async fn estados(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[i64],
    ) -> Result<Vec<EstadoMensaje>, ErrorPersistencia> {
        let filas: Vec<FilaEstado> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNAS_ESTADO} FROM notificaciones.mensaje WHERE id = ANY($1) ORDER BY id"
        )))
        .bind(ids)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(EstadoMensaje::from).collect())
    }
}

/// La cola vista por el worker.
#[derive(Debug)]
pub(crate) struct RepositorioMensajesPg {
    pool: PgPool,
}

impl RepositorioMensajesPg {
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
    creado_en: DateTime<Utc>,
}

/// Borra las variables de los mensajes secretos, salvo que se pidan conservar (simulador).
const BORRAR_SECRETOS: &str =
    "variables = CASE WHEN contiene_secreto AND NOT $3 THEN '{}'::jsonb ELSE variables END";

#[async_trait]
impl RepositorioMensajes for RepositorioMensajesPg {
    async fn reservar(
        &self,
        canal: Canal,
        ahora: DateTime<Utc>,
        reservar_hasta: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<MensajePendiente>, ErrorPersistencia> {
        // SKIP LOCKED: dos workers nunca toman el mismo mensaje. Mover `programado_para` es la
        // reserva: si el worker muere sin marcar el resultado, el mensaje vuelve a estar
        // disponible cuando vence.
        let filas: Vec<FilaPendiente> = sqlx::query_as(
            "WITH lote AS (
                 SELECT id FROM notificaciones.mensaje
                 WHERE estado = 'pendiente' AND canal = $4::notificaciones.canal AND programado_para <= $1
                 ORDER BY programado_para
                 LIMIT $3
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE notificaciones.mensaje m
             SET programado_para = $2, intentos = m.intentos + 1
             FROM lote
             WHERE m.id = lote.id
             RETURNING m.id, m.destinatario, m.plantilla, m.variables, m.intentos, m.creado_en",
        )
        .bind(ahora)
        .bind(reservar_hasta)
        .bind(limite)
        .bind(canal.codigo())
        .fetch_all(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|f| MensajePendiente {
                id: f.id,
                destinatario: f.destinatario,
                plantilla: f.plantilla,
                variables: f.variables,
                intentos: f.intentos,
                creado_en: f.creado_en,
            })
            .collect())
    }

    async fn marcar_enviado(
        &self,
        id: i64,
        ahora: DateTime<Utc>,
        id_proveedor: Option<&str>,
        conservar_variables: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE notificaciones.mensaje
             SET estado = 'enviado', enviado_en = $2, ultimo_error = NULL, id_proveedor = $4, {BORRAR_SECRETOS}
             WHERE id = $1"
        )))
        .bind(id)
        .bind(ahora)
        .bind(conservar_variables)
        .bind(id_proveedor)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn reintentar(&self, id: i64, error: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE notificaciones.mensaje SET ultimo_error = $2, programado_para = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(recortar(error))
        .bind(cuando)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn marcar_vencido(
        &self,
        id: i64,
        motivo: &str,
        conservar_variables: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE notificaciones.mensaje
             SET estado = 'cancelado', ultimo_error = $2, {BORRAR_SECRETOS}
             WHERE id = $1"
        )))
        .bind(id)
        .bind(recortar(motivo))
        .bind(conservar_variables)
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn marcar_fallido(
        &self,
        id: i64,
        ahora: DateTime<Utc>,
        error: &str,
        conservar_variables: bool,
        aviso: Option<NuevoAviso>,
    ) -> Result<(), ErrorPersistencia> {
        // Un enlace con token que no se pudo entregar tampoco debe quedarse guardado.
        let mut tx = self.pool.begin().await.map_err(a_error_persistencia)?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE notificaciones.mensaje
             SET estado = 'fallido', ultimo_error = $2, {BORRAR_SECRETOS}
             WHERE id = $1"
        )))
        .bind(id)
        .bind(recortar(error))
        .bind(conservar_variables)
        .execute(&mut *tx)
        .await
        .map_err(a_error_persistencia)?;
        if let Some(aviso) = aviso {
            insertar_aviso(&mut tx, &aviso, ahora).await?;
        }
        tx.commit().await.map_err(a_error_persistencia)
    }
}

fn recortar(error: &str) -> String {
    error.chars().take(500).collect()
}

/// Webhooks de WhatsApp en la transacción del procesamiento.
#[derive(Debug, Default)]
pub(crate) struct RepositorioWebhooksPg;

#[derive(sqlx::FromRow)]
struct FilaRegistrado {
    id: i64,
    estado: String,
    plantilla: String,
    variables: Value,
    destinatario: String,
}

#[async_trait]
impl RepositorioWebhooks<UnidadDeTrabajo> for RepositorioWebhooksPg {
    async fn tomar(&self, uow: &mut UnidadDeTrabajo) -> Result<Option<EventoWebhook>, ErrorPersistencia> {
        let fila: Option<(i64, Value)> = sqlx::query_as(
            "SELECT id, carga FROM notificaciones.evento_webhook
             WHERE procesado_en IS NULL ORDER BY recibido_en, id LIMIT 1
             FOR UPDATE SKIP LOCKED",
        )
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|(id, carga)| EventoWebhook { id, carga }))
    }

    async fn marcar_procesado(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
        ahora: DateTime<Utc>,
        error: Option<&str>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE notificaciones.evento_webhook SET procesado_en = $2, error = $3 WHERE id = $1")
            .bind(id)
            .bind(ahora)
            .bind(error.map(recortar))
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn mensaje_por_proveedor(
        &self,
        uow: &mut UnidadDeTrabajo,
        id_proveedor: &str,
    ) -> Result<Option<MensajeRegistrado>, ErrorPersistencia> {
        let fila: Option<FilaRegistrado> = sqlx::query_as(
            "SELECT id, estado::text AS estado, plantilla, variables, destinatario
             FROM notificaciones.mensaje WHERE id_proveedor = $1
             FOR UPDATE",
        )
        .bind(id_proveedor)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|f| MensajeRegistrado {
            id: f.id,
            estado: f.estado,
            plantilla: f.plantilla,
            variables: f.variables,
            destinatario: f.destinatario,
        }))
    }

    async fn cambiar_estado(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
        estado: EstadoEntrega,
        error: Option<&str>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE notificaciones.mensaje
             SET estado = $2::notificaciones.estado_mensaje,
                 ultimo_error = coalesce($3, ultimo_error),
                 enviado_en = coalesce(enviado_en, CASE WHEN $2 = 'fallido' THEN NULL ELSE $4 END),
                 variables = CASE WHEN $2 = 'fallido' AND contiene_secreto THEN '{}'::jsonb ELSE variables END
             WHERE id = $1",
        )
        .bind(id)
        .bind(estado.codigo())
        .bind(error.map(recortar))
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}

/// Guarda un webhook recibido (lo procesa el worker).
pub(crate) async fn guardar_webhook(
    pool: &PgPool,
    carga: &Value,
    ahora: DateTime<Utc>,
) -> Result<i64, ErrorPersistencia> {
    sqlx::query_scalar(
        "INSERT INTO notificaciones.evento_webhook (carga, recibido_en) VALUES ($1, $2) RETURNING id",
    )
    .bind(carga)
    .bind(ahora)
    .fetch_one(pool)
    .await
    .map_err(a_error_persistencia)
}

/// Aviso interno tal como lo ve la administradora.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct AvisoDto {
    pub(crate) id: i64,
    pub(crate) tipo: String,
    pub(crate) titulo: String,
    pub(crate) detalle: Option<String>,
    pub(crate) datos: Value,
    pub(crate) creado_en: DateTime<Utc>,
    pub(crate) leido_en: Option<DateTime<Utc>>,
}

const COLUMNAS_AVISO: &str = "id, tipo::text AS tipo, titulo, detalle, datos, creado_en, leido_en";

pub(crate) async fn listar_avisos(
    pool: &PgPool,
    solo_no_leidos: bool,
    pagina: SolicitudPagina,
) -> Result<Pagina<AvisoDto>, ErrorPersistencia> {
    let filas: Vec<AvisoDto> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNAS_AVISO} FROM notificaciones.aviso
         WHERE (NOT $1 OR leido_en IS NULL)
         ORDER BY creado_en DESC, id DESC LIMIT $2 OFFSET $3"
    )))
    .bind(solo_no_leidos)
    .bind(pagina.limite())
    .bind(pagina.desplazamiento())
    .fetch_all(pool)
    .await
    .map_err(a_error_persistencia)?;
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notificaciones.aviso WHERE (NOT $1 OR leido_en IS NULL)")
            .bind(solo_no_leidos)
            .fetch_one(pool)
            .await
            .map_err(a_error_persistencia)?;
    Ok(Pagina::nueva(filas, pagina, total))
}

/// Marca el aviso como leído (idempotente: conserva la primera lectura).
pub(crate) async fn marcar_aviso_leido(
    pool: &PgPool,
    id: i64,
    usuario: i64,
    ahora: DateTime<Utc>,
) -> Result<Option<AvisoDto>, ErrorPersistencia> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE notificaciones.aviso
         SET leido_en = coalesce(leido_en, $3), leido_por = coalesce(leido_por, $2)
         WHERE id = $1
         RETURNING {COLUMNAS_AVISO}"
    )))
    .bind(id)
    .bind(usuario)
    .bind(ahora)
    .fetch_optional(pool)
    .await
    .map_err(a_error_persistencia)
}

/// Registro de mensajes para la administradora (RF-NOT-07): sin variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub(crate) struct MensajeDto {
    pub(crate) id: i64,
    pub(crate) canal: String,
    pub(crate) plantilla: String,
    pub(crate) destinatario: String,
    pub(crate) estado: String,
    pub(crate) intentos: i16,
    pub(crate) ultimo_error: Option<String>,
    pub(crate) programado_para: DateTime<Utc>,
    pub(crate) creado_en: DateTime<Utc>,
    pub(crate) enviado_en: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FiltroMensajes {
    pub(crate) destinatario: Option<String>,
    pub(crate) estado: Option<String>,
    pub(crate) canal: Option<String>,
    pub(crate) plantilla: Option<String>,
}

pub(crate) async fn listar_mensajes(
    pool: &PgPool,
    filtro: &FiltroMensajes,
    pagina: SolicitudPagina,
) -> Result<Pagina<MensajeDto>, ErrorPersistencia> {
    const CONDICION: &str = "($1::text IS NULL OR destinatario = $1)
         AND ($2::text IS NULL OR estado::text = $2)
         AND ($3::text IS NULL OR canal::text = $3)
         AND ($4::text IS NULL OR plantilla = $4)";
    let filas: Vec<MensajeDto> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, canal::text AS canal, plantilla, destinatario, estado::text AS estado, intentos, ultimo_error,
                programado_para, creado_en, enviado_en
         FROM notificaciones.mensaje WHERE {CONDICION}
         ORDER BY creado_en DESC, id DESC LIMIT $5 OFFSET $6"
    )))
    .bind(&filtro.destinatario)
    .bind(&filtro.estado)
    .bind(&filtro.canal)
    .bind(&filtro.plantilla)
    .bind(pagina.limite())
    .bind(pagina.desplazamiento())
    .fetch_all(pool)
    .await
    .map_err(a_error_persistencia)?;
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM notificaciones.mensaje WHERE {CONDICION}"
    )))
    .bind(&filtro.destinatario)
    .bind(&filtro.estado)
    .bind(&filtro.canal)
    .bind(&filtro.plantilla)
    .fetch_one(pool)
    .await
    .map_err(a_error_persistencia)?;
    Ok(Pagina::nueva(filas, pagina, total))
}

/// Mantenimiento: webhooks ya procesados hace más de 30 días.
pub(crate) async fn limpiar(pool: &PgPool, ahora: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
    sqlx::query("DELETE FROM notificaciones.evento_webhook WHERE procesado_en < $1")
        .bind(ahora - TimeDelta::days(30))
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(a_error_persistencia)
}

/// Mensajes recientes de un destinatario, para las pruebas (WhatsApp simulado).
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub(crate) struct MensajeDePrueba {
    id: i64,
    canal: String,
    plantilla: String,
    destinatario: String,
    variables: Value,
    estado: String,
    id_proveedor: Option<String>,
    creado_en: DateTime<Utc>,
}

pub(crate) async fn mensajes_recientes(
    pool: &PgPool,
    destinatario: &str,
) -> Result<Vec<MensajeDePrueba>, ErrorPersistencia> {
    sqlx::query_as(
        "SELECT id, canal::text AS canal, plantilla, destinatario, variables, estado::text AS estado, id_proveedor,
                creado_en
         FROM notificaciones.mensaje WHERE destinatario = $1 ORDER BY id DESC LIMIT 20",
    )
    .bind(destinatario)
    .fetch_all(pool)
    .await
    .map_err(a_error_persistencia)
}

/// Estado de la cola y de los webhooks a la hora `ahora` (RNF-OBS-02). Recorre solo lo pendiente
/// y lo de las últimas 24 horas.
pub(crate) async fn estado_cola(
    pool: &PgPool,
    ahora: DateTime<Utc>,
) -> Result<(Vec<(String, crate::api::EstadoCanal)>, (i64, Option<i64>, i64)), ErrorPersistencia> {
    let canales: Vec<(String, i64, Option<i64>, i64, i64, i64)> = sqlx::query_as(
        "SELECT canal::text,
                count(*) FILTER (WHERE estado = 'pendiente' AND programado_para <= $1),
                (extract(epoch FROM $1 - min(programado_para) FILTER (WHERE estado = 'pendiente' AND programado_para <= $1)) / 60)::bigint,
                count(*) FILTER (WHERE enviado_en >= $1 - interval '24 hours'),
                count(*) FILTER (WHERE estado = 'fallido' AND actualizado_en >= $1 - interval '24 hours'),
                count(*) FILTER (WHERE estado = 'fallido' AND actualizado_en >= $1 - interval '1 hour')
         FROM notificaciones.mensaje
         WHERE (estado = 'pendiente' AND programado_para <= $1) OR actualizado_en >= $1 - interval '24 hours'
         GROUP BY canal",
    )
    .bind(ahora)
    .fetch_all(pool)
    .await
    .map_err(a_error_persistencia)?;
    let webhooks: (i64, Option<i64>, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE procesado_en IS NULL),
                (extract(epoch FROM $1 - min(recibido_en) FILTER (WHERE procesado_en IS NULL)) / 60)::bigint,
                count(*) FILTER (WHERE error IS NOT NULL AND procesado_en >= $1 - interval '24 hours')
         FROM notificaciones.evento_webhook
         WHERE procesado_en IS NULL OR procesado_en >= $1 - interval '24 hours'",
    )
    .bind(ahora)
    .fetch_one(pool)
    .await
    .map_err(a_error_persistencia)?;
    Ok((
        canales
            .into_iter()
            .map(|(canal, pendientes, atraso, enviados, fallidos, fallidos_hora)| {
                (
                    canal,
                    crate::api::EstadoCanal {
                        configurado: false,
                        pendientes,
                        atraso_min: atraso,
                        enviados_24h: enviados,
                        fallidos_24h: fallidos,
                        fallidos_ultima_hora: fallidos_hora,
                    },
                )
            })
            .collect(),
        webhooks,
    ))
}
