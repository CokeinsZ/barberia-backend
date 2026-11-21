//! Repositorio del esquema `agenda`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio};
use nucleo::{ErrorPersistencia, TipoActor};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use plataforma::seguridad::Huella;

use crate::aplicacion::dto::EventoCitaDto;
use crate::aplicacion::puertos::{
    CitaGuardada, CitaParaRecordar, EventoCita, FiltroCitas, NuevaCita, RepositorioAgenda, local,
};
use crate::dominio::cita::EstadoCita;
use crate::dominio::configuracion::ConfiguracionAgenda;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioAgendaPg;

const COLUMNAS: &str = "id, cliente_id, barbero_id, servicio_id, inicio, fin, precio::bigint AS precio, estado::text AS estado,
     origen::text AS origen, asignacion_automatica, nota, cancelacion_tardia, motivo_cancelacion::text AS motivo_cancelacion";

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    cliente_id: i64,
    barbero_id: i64,
    servicio_id: i64,
    inicio: DateTime<Utc>,
    fin: DateTime<Utc>,
    precio: i64,
    estado: String,
    origen: String,
    asignacion_automatica: bool,
    nota: Option<String>,
    cancelacion_tardia: Option<bool>,
    motivo_cancelacion: Option<String>,
}

/// Las mismas columnas de `COLUMNAS`, calificadas con el alias `c`.
const COLUMNAS_C: &str = "c.id, c.cliente_id, c.barbero_id, c.servicio_id, c.inicio, c.fin, c.precio::bigint AS precio,
     c.estado::text AS estado, c.origen::text AS origen, c.asignacion_automatica, c.nota, c.cancelacion_tardia,
     c.motivo_cancelacion::text AS motivo_cancelacion";

#[derive(sqlx::FromRow)]
struct FilaRecordar {
    #[sqlx(flatten)]
    cita: Fila,
    vigente_desde: DateTime<Utc>,
    enviados: Vec<String>,
}

impl TryFrom<Fila> for CitaGuardada {
    type Error = ErrorPersistencia;

    fn try_from(f: Fila) -> Result<Self, Self::Error> {
        Ok(CitaGuardada {
            id: IdCita(f.id),
            cliente: IdCliente(f.cliente_id),
            barbero: IdBarbero(f.barbero_id),
            servicio: IdServicio(f.servicio_id),
            inicio: f.inicio,
            fin: f.fin,
            precio: f.precio,
            estado: EstadoCita::desde_codigo(&f.estado).ok_or_else(|| {
                ErrorPersistencia::Otro(format!("estado de cita desconocido: {}", f.estado))
            })?,
            origen: f.origen,
            asignacion_automatica: f.asignacion_automatica,
            nota: f.nota,
            cancelacion_tardia: f.cancelacion_tardia,
            motivo_cancelacion: f.motivo_cancelacion,
        })
    }
}

#[async_trait]
impl RepositorioAgenda<UnidadDeTrabajo> for RepositorioAgendaPg {
    async fn configuracion(
        &self,
        uow: &mut UnidadDeTrabajo,
    ) -> Result<ConfiguracionAgenda, ErrorPersistencia> {
        let (a, v, i, aviso, r24, r2, politica, umbral): (i16, i16, i16, i16, bool, bool, String, i16) = sqlx::query_as(
            "SELECT anticipacion_minima_min, ventana_reserva_dias, intervalo_min, aviso_cancelacion_min, recordatorio_24h,
                    recordatorio_2h, politica_inasistencias::text, umbral_inasistencias
             FROM agenda.configuracion",
        )
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(ConfiguracionAgenda {
            anticipacion_minima_min: a,
            ventana_reserva_dias: v,
            intervalo_min: i,
            aviso_cancelacion_min: aviso,
            recordatorio_24h: r24,
            recordatorio_2h: r2,
            politica_inasistencias: politica,
            umbral_inasistencias: umbral,
        })
    }

    async fn guardar_configuracion(
        &self,
        uow: &mut UnidadDeTrabajo,
        c: &ConfiguracionAgenda,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE agenda.configuracion
             SET anticipacion_minima_min = $1, ventana_reserva_dias = $2, intervalo_min = $3, aviso_cancelacion_min = $4,
                 recordatorio_24h = $5, recordatorio_2h = $6, politica_inasistencias = $7::agenda.politica_inasistencias,
                 umbral_inasistencias = $8",
        )
        .bind(c.anticipacion_minima_min)
        .bind(c.ventana_reserva_dias)
        .bind(c.intervalo_min)
        .bind(c.aviso_cancelacion_min)
        .bind(c.recordatorio_24h)
        .bind(c.recordatorio_2h)
        .bind(&c.politica_inasistencias)
        .bind(c.umbral_inasistencias)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn ocupacion(
        &self,
        uow: &mut UnidadDeTrabajo,
        barberos: &[IdBarbero],
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
        excepto: Option<IdCita>,
    ) -> Result<Vec<(IdBarbero, DateTime<Utc>, DateTime<Utc>)>, ErrorPersistencia> {
        let ids: Vec<i64> = barberos.iter().map(|b| b.0).collect();
        let filas: Vec<(i64, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT barbero_id, inicio, fin FROM agenda.cita
             WHERE barbero_id = ANY($1) AND estado NOT IN ('cancelada', 'no_asistio')
               AND inicio < $3 AND fin > $2 AND ($4::bigint IS NULL OR id <> $4)",
        )
        .bind(&ids)
        .bind(desde)
        .bind(hasta)
        .bind(excepto.map(|c| c.0))
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(|(b, i, f)| (IdBarbero(b), i, f)).collect())
    }

    async fn crear(&self, uow: &mut UnidadDeTrabajo, c: &NuevaCita) -> Result<IdCita, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen,
                                      asignacion_automatica, nota, creada_por, creada_en, actualizado_en)
             VALUES ($1, $2, $3, $4, $5, $6, $7::agenda.estado_cita,
                     (CASE WHEN $10::bigint IS NULL THEN 'en_linea' ELSE 'panel' END)::agenda.origen_cita,
                     $8, $9, $10, $11, $11)
             RETURNING id",
        )
        .bind(c.cliente.0)
        .bind(c.barbero.0)
        .bind(c.servicio.0)
        .bind(c.inicio)
        .bind(c.fin)
        .bind(c.precio)
        .bind(c.estado.codigo())
        .bind(c.asignacion_automatica)
        .bind(&c.nota)
        .bind(c.creada_por.map(|u| u.0))
        .bind(c.ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdCita(id))
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCita,
        bloquear: bool,
    ) -> Result<Option<CitaGuardada>, ErrorPersistencia> {
        let consulta = if bloquear {
            format!("SELECT {COLUMNAS} FROM agenda.cita WHERE id = $1 FOR UPDATE")
        } else {
            format!("SELECT {COLUMNAS} FROM agenda.cita WHERE id = $1")
        };
        // La consulta se arma solo con constantes de este archivo.
        let fila: Option<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(id.0)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        fila.map(CitaGuardada::try_from).transpose()
    }

    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        f: &FiltroCitas,
    ) -> Result<Vec<CitaGuardada>, ErrorPersistencia> {
        let estados: Vec<&str> = f.estados.iter().map(|e| e.codigo()).collect();
        let consulta = format!(
            "SELECT {COLUMNAS} FROM agenda.cita
             WHERE ($1::timestamptz IS NULL OR fin > $1)
               AND ($2::timestamptz IS NULL OR inicio < $2)
               AND ($3::bigint IS NULL OR barbero_id = $3)
               AND (cardinality($4::text[]) = 0 OR estado::text = ANY($4))
             ORDER BY inicio, id
             LIMIT 2000"
        );
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(f.desde)
            .bind(f.hasta)
            .bind(f.barbero.map(|b| b.0))
            .bind(&estados)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        filas.into_iter().map(CitaGuardada::try_from).collect()
    }

    async fn cambiar_estado(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCita,
        estado: EstadoCita,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE agenda.cita SET estado = $2::agenda.estado_cita WHERE id = $1")
            .bind(id.0)
            .bind(estado.codigo())
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn cancelar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCita,
        por: TipoActor,
        motivo: &str,
        tardia: bool,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE agenda.cita
             SET estado = 'cancelada', cancelada_en = $5, cancelada_por = $2::nucleo.actor,
                 motivo_cancelacion = $3::agenda.motivo_cancelacion, cancelacion_tardia = $4
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(por.como_str())
        .bind(motivo)
        .bind(tardia)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn mover(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCita,
        barbero: IdBarbero,
        inicio: DateTime<Utc>,
        fin: DateTime<Utc>,
        asignacion_automatica: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE agenda.cita SET barbero_id = $2, inicio = $3, fin = $4, asignacion_automatica = $5 WHERE id = $1")
            .bind(id.0)
            .bind(barbero.0)
            .bind(inicio)
            .bind(fin)
            .bind(asignacion_automatica)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn registrar_evento(
        &self,
        uow: &mut UnidadDeTrabajo,
        e: &EventoCita,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO agenda.historial_cita (cita_id, evento, actor, usuario_id, inicio_previo, barbero_previo, tardio, detalle, ocurrido_en)
             VALUES ($1, $2::agenda.evento_cita, $3::nucleo.actor, $4, $5, $6, $7, $8, $9)",
        )
        .bind(e.cita.0)
        .bind(e.evento)
        .bind(e.actor.como_str())
        .bind(e.usuario.map(|u| u.0))
        .bind(e.inicio_previo)
        .bind(e.barbero_previo.map(|b| b.0))
        .bind(e.tardio)
        .bind(&e.detalle)
        .bind(e.ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn historial(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCita,
    ) -> Result<Vec<EventoCitaDto>, ErrorPersistencia> {
        let filas: Vec<(
            String,
            String,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<i64>,
            bool,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT evento::text, actor::text, ocurrido_en, inicio_previo, barbero_previo, tardio, detalle
             FROM agenda.historial_cita WHERE cita_id = $1 ORDER BY ocurrido_en, id",
        )
        .bind(id.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(
                |(evento, actor, ocurrido_en, inicio_previo, barbero_previo, tardio, detalle)| {
                    EventoCitaDto {
                        evento,
                        actor,
                        ocurrido_en: local(ocurrido_en),
                        inicio_previo: inicio_previo.map(local),
                        barbero_previo: barbero_previo.map(IdBarbero),
                        tardio,
                        detalle,
                    }
                },
            )
            .collect())
    }

    async fn guardar_enlace(
        &self,
        uow: &mut UnidadDeTrabajo,
        cita: IdCita,
        huella: &Huella,
        expira_en: DateTime<Utc>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO agenda.enlace_gestion (cita_id, token_hash, expira_en, creado_en) VALUES ($1, $2, $3, $4)
             ON CONFLICT (cita_id) DO UPDATE
             SET token_hash = EXCLUDED.token_hash, expira_en = EXCLUDED.expira_en, creado_en = EXCLUDED.creado_en",
        )
        .bind(cita.0)
        .bind(&huella[..])
        .bind(expira_en)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn por_enlace(
        &self,
        uow: &mut UnidadDeTrabajo,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<IdCita>, ErrorPersistencia> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT cita_id FROM agenda.enlace_gestion WHERE token_hash = $1 AND expira_en > $2",
        )
        .bind(&huella[..])
        .bind(ahora)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(id.map(IdCita))
    }

    async fn vincular_mensaje(
        &self,
        uow: &mut UnidadDeTrabajo,
        cita: IdCita,
        proposito: &'static str,
        inicio: DateTime<Utc>,
        mensaje: i64,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO agenda.mensaje_cita (cita_id, proposito, inicio_cita, mensaje_id) VALUES ($1, $2::agenda.proposito_mensaje, $3, $4)",
        )
        .bind(cita.0)
        .bind(proposito)
        .bind(inicio)
        .bind(mensaje)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn citas_para_recordar(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
    ) -> Result<Vec<CitaParaRecordar>, ErrorPersistencia> {
        // `vigente_desde`: cuándo quedó la cita en su hora actual (creación o último cambio de
        // hora). Un cambio solo de barbero no cuenta: la hora sigue siendo la misma.
        let consulta = format!(
            "SELECT {COLUMNAS_C},
                    greatest(c.creada_en, coalesce((
                        SELECT max(h.ocurrido_en) FROM agenda.historial_cita h
                        WHERE h.cita_id = c.id AND h.evento IN ('reprogramada', 'reasignada')
                          AND h.inicio_previo IS DISTINCT FROM c.inicio
                    ), c.creada_en)) AS vigente_desde,
                    coalesce((
                        SELECT array_agg(m.proposito::text) FROM agenda.mensaje_cita m
                        WHERE m.cita_id = c.id AND m.inicio_cita = c.inicio
                          AND m.proposito IN ('recordatorio_24h', 'recordatorio_2h')
                    ), '{{}}') AS enviados
             FROM agenda.cita c
             WHERE c.estado IN ('sin_confirmar', 'confirmada') AND c.inicio > $1 AND c.inicio <= $2
             ORDER BY c.inicio, c.id"
        );
        let filas: Vec<FilaRecordar> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(desde)
            .bind(hasta)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        filas
            .into_iter()
            .map(|f| {
                Ok(CitaParaRecordar {
                    vigente_desde: f.vigente_desde,
                    recordatorios_enviados: f.enviados,
                    cita: CitaGuardada::try_from(f.cita)?,
                })
            })
            .collect()
    }

    async fn mensajes_de_cita(
        &self,
        uow: &mut UnidadDeTrabajo,
        cita: IdCita,
    ) -> Result<Vec<(String, i64)>, ErrorPersistencia> {
        sqlx::query_as(
            "SELECT proposito::text, mensaje_id FROM agenda.mensaje_cita WHERE cita_id = $1 ORDER BY mensaje_id",
        )
        .bind(cita.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn pendientes_de_cliente(
        &self,
        uow: &mut UnidadDeTrabajo,
        cliente: IdCliente,
    ) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar(
            "SELECT count(*) FROM agenda.cita
             WHERE cliente_id = $1 AND estado IN ('sin_confirmar', 'confirmada', 'en_curso')",
        )
        .bind(cliente.0)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn cita_por_mensaje(
        &self,
        uow: &mut UnidadDeTrabajo,
        mensaje: i64,
    ) -> Result<Option<(IdCita, String)>, ErrorPersistencia> {
        let fila: Option<(i64, String)> =
            sqlx::query_as("SELECT cita_id, proposito::text FROM agenda.mensaje_cita WHERE mensaje_id = $1")
                .bind(mensaje)
                .fetch_optional(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(fila.map(|(cita, proposito)| (IdCita(cita), proposito)))
    }
}

/// Mantenimiento: enlaces de gestión vencidos (la cita ya pasó hace más de 30 días).
pub(crate) async fn limpiar(pool: &sqlx::PgPool, ahora: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
    sqlx::query("DELETE FROM agenda.enlace_gestion WHERE expira_en < $1")
        .bind(ahora)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
        .map_err(a_error_persistencia)
}
