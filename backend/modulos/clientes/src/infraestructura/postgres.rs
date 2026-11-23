use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::identificadores::IdCliente;
use nucleo::{Celular, ErrorPersistencia, SolicitudPagina};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::api::{ClienteConocido, DatosReservaEnLinea};
use crate::aplicacion::dto::{AutorizacionDto, ConteoSegmentosDto, IndicadoresClientesDto};
use crate::aplicacion::puertos::{
    ClienteGuardado, RepositorioClientes, RepositorioTitular, SolicitudGuardada,
};
use crate::dominio::{DatosCliente, UmbralesSegmentos};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioClientesPg;

/// Métricas de la vista publicada `clientes.v_cliente_resumen` (excluye anonimizados).
const SELECCION: &str = "
    SELECT r.id, r.celular::text AS celular, r.nombre, r.correo::text AS correo, r.fecha_nacimiento,
           r.visitas, r.total_gastado::bigint AS total_gastado, r.ultima_visita, r.inasistencias,
           r.barbero_habitual_id, c.preferencias, c.recordatorios_activos, c.creado_en,
           count(*) OVER () AS total
    FROM clientes.v_cliente_resumen r
    JOIN clientes.cliente c ON c.id = r.id";

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    celular: String,
    nombre: String,
    correo: Option<String>,
    fecha_nacimiento: Option<NaiveDate>,
    visitas: i32,
    total_gastado: i64,
    ultima_visita: Option<NaiveDate>,
    inasistencias: i32,
    barbero_habitual_id: Option<i64>,
    preferencias: Option<String>,
    recordatorios_activos: bool,
    creado_en: DateTime<Utc>,
    total: i64,
}

impl From<Fila> for ClienteGuardado {
    fn from(f: Fila) -> Self {
        ClienteGuardado {
            conocido: ClienteConocido {
                id: IdCliente(f.id),
                nombre: f.nombre,
                celular: f.celular,
                correo: f.correo,
                fecha_nacimiento: f.fecha_nacimiento,
                visitas: f.visitas,
                inasistencias: f.inasistencias,
                recordatorios_activos: f.recordatorios_activos,
            },
            total_gastado: f.total_gastado,
            ultima_visita: f.ultima_visita,
            barbero_habitual: f.barbero_habitual_id,
            preferencias: f.preferencias,
            creado_en: f.creado_en,
        }
    }
}

#[async_trait]
impl RepositorioClientes<UnidadDeTrabajo> for RepositorioClientesPg {
    async fn por_celular(
        &self,
        uow: &mut UnidadDeTrabajo,
        celular: &Celular,
    ) -> Result<Option<ClienteGuardado>, ErrorPersistencia> {
        let consulta = format!("{SELECCION} WHERE r.celular = $1");
        // La consulta se arma solo con constantes de este archivo.
        let fila: Option<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(celular.digitos())
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(fila.map(ClienteGuardado::from))
    }

    async fn por_ids(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[IdCliente],
    ) -> Result<Vec<ClienteGuardado>, ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        let consulta = format!("{SELECCION} WHERE r.id = ANY($1)");
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(&ids)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(ClienteGuardado::from).collect())
    }

    async fn registrar_en_linea(
        &self,
        uow: &mut UnidadDeTrabajo,
        d: &DatosReservaEnLinea,
        ahora: DateTime<Utc>,
    ) -> Result<IdCliente, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO clientes.cliente (celular, nombre, correo, fecha_nacimiento)
             VALUES ($1, $2, $3::citext, $4)
             ON CONFLICT (celular) DO UPDATE SET
                 nombre = EXCLUDED.nombre,
                 correo = coalesce(EXCLUDED.correo, clientes.cliente.correo),
                 fecha_nacimiento = coalesce(EXCLUDED.fecha_nacimiento, clientes.cliente.fecha_nacimiento)
             RETURNING id",
        )
        .bind(d.celular.digitos())
        .bind(&d.nombre)
        .bind(&d.correo)
        .bind(d.fecha_nacimiento)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        sqlx::query(
            "INSERT INTO clientes.autorizacion (cliente_id, politica_version_id, canal, otorgada_en, ip, agente_usuario)
             VALUES ($1, $2, 'en_linea', $3, $4::inet, $5)",
        )
        .bind(id)
        .bind(i16::try_from(d.politica_version_id).unwrap_or(i16::MAX))
        .bind(ahora)
        .bind(d.ip.map(|ip| ip.to_string()))
        .bind(d.agente.as_deref().map(|a| a.chars().take(300).collect::<String>()))
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdCliente(id))
    }

    async fn crear(
        &self,
        uow: &mut UnidadDeTrabajo,
        d: &DatosCliente,
    ) -> Result<IdCliente, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO clientes.cliente (celular, nombre, correo, fecha_nacimiento) VALUES ($1, $2, $3::citext, $4) RETURNING id",
        )
        .bind(d.celular.digitos())
        .bind(&d.nombre)
        .bind(d.correo.as_ref().map(|c| c.como_str()))
        .bind(d.fecha_nacimiento)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdCliente(id))
    }

    async fn actualizar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCliente,
        d: &DatosCliente,
        preferencias: Option<&str>,
        recordatorios_activos: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE clientes.cliente
             SET celular = $2, nombre = $3, correo = $4::citext, fecha_nacimiento = $5, preferencias = $6, recordatorios_activos = $7
             WHERE id = $1 AND anonimizado_en IS NULL",
        )
        .bind(id.0)
        .bind(d.celular.digitos())
        .bind(&d.nombre)
        .bind(d.correo.as_ref().map(|c| c.como_str()))
        .bind(d.fecha_nacimiento)
        .bind(preferencias)
        .bind(recordatorios_activos)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn cambiar_recordatorios(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCliente,
        activos: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE clientes.cliente SET recordatorios_activos = $2 WHERE id = $1")
            .bind(id.0)
            .bind(activos)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn buscar(
        &self,
        uow: &mut UnidadDeTrabajo,
        texto: Option<&str>,
        pagina: Option<SolicitudPagina>,
    ) -> Result<(Vec<ClienteGuardado>, i64), ErrorPersistencia> {
        // `strpos` y no `LIKE`: así un `%` o un `_` escrito por la persona no es un comodín.
        let consulta = format!(
            "{SELECCION}
             WHERE $1::text IS NULL
                OR strpos(nucleo.normalizar(r.nombre), nucleo.normalizar($1)) > 0
                OR (regexp_replace($1, '\\D', '', 'g') <> '' AND strpos(r.celular::text, regexp_replace($1, '\\D', '', 'g')) > 0)
                OR strpos(lower(coalesce(r.correo::text, '')), lower($1)) > 0
             ORDER BY c.creado_en DESC, r.id DESC
             LIMIT $2 OFFSET $3"
        );
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(texto)
            // `LIMIT NULL` es «sin límite».
            .bind(pagina.map(SolicitudPagina::limite))
            .bind(pagina.map_or(0, SolicitudPagina::desplazamiento))
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        let total = filas.first().map_or(0, |f| f.total);
        Ok((filas.into_iter().map(ClienteGuardado::from).collect(), total))
    }

    async fn autorizaciones(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCliente,
    ) -> Result<Vec<AutorizacionDto>, ErrorPersistencia> {
        let filas: Vec<(i16, String, DateTime<Utc>, bool)> = sqlx::query_as(
            "SELECT politica_version_id, canal::text, otorgada_en, revocada_en IS NOT NULL
             FROM clientes.autorizacion WHERE cliente_id = $1 ORDER BY otorgada_en DESC",
        )
        .bind(id.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(politica, canal, otorgada_en, revocada)| AutorizacionDto {
                politica_version_id: i64::from(politica),
                canal,
                otorgada_en,
                revocada,
            })
            .collect())
    }

    async fn umbrales(&self, uow: &mut UnidadDeTrabajo) -> Result<UmbralesSegmentos, ErrorPersistencia> {
        let (nuevo, recurrente, dias): (i16, i16, i16) = sqlx::query_as(
            "SELECT max_visitas_nuevo, max_visitas_recurrente, dias_inactividad FROM clientes.configuracion_segmentos",
        )
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(UmbralesSegmentos {
            max_visitas_nuevo: i32::from(nuevo),
            max_visitas_recurrente: i32::from(recurrente),
            dias_inactividad: i64::from(dias),
        })
    }

    async fn guardar_umbrales(
        &self,
        uow: &mut UnidadDeTrabajo,
        u: &UmbralesSegmentos,
    ) -> Result<(), ErrorPersistencia> {
        // Ya validados (`UmbralesSegmentos::nuevos`): caben en `smallint`.
        let a_smallint = |v: i64| i16::try_from(v).unwrap_or(i16::MAX);
        sqlx::query(
            "UPDATE clientes.configuracion_segmentos
             SET max_visitas_nuevo = $1, max_visitas_recurrente = $2, dias_inactividad = $3
             WHERE id = 1",
        )
        .bind(a_smallint(i64::from(u.max_visitas_nuevo)))
        .bind(a_smallint(i64::from(u.max_visitas_recurrente)))
        .bind(a_smallint(u.dias_inactividad))
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn indicadores(
        &self,
        uow: &mut UnidadDeTrabajo,
        hoy: NaiveDate,
        u: UmbralesSegmentos,
    ) -> Result<IndicadoresClientesDto, ErrorPersistencia> {
        // Igual que `dominio::segmento`: sin visitas; «sin volver» si la última visita fue
        // hace más de `dias_inactividad` días (prevalece); si no, por número de visitas.
        let (registrados, sin_visitas, inactivo, nuevo, recurrente, frecuente): (i64, i64, i64, i64, i64, i64) =
            sqlx::query_as(
                "WITH r AS (
                   SELECT visitas, visitas > 0 AND coalesce(ultima_visita < $1::date - $4::int, false) AS sin_volver
                   FROM clientes.v_cliente_resumen
                 )
                 SELECT count(*),
                        count(*) FILTER (WHERE visitas = 0),
                        count(*) FILTER (WHERE sin_volver),
                        count(*) FILTER (WHERE visitas > 0 AND NOT sin_volver AND visitas <= $2),
                        count(*) FILTER (WHERE NOT sin_volver AND visitas > $2 AND visitas <= $3),
                        count(*) FILTER (WHERE NOT sin_volver AND visitas > $3)
                 FROM r",
            )
            .bind(hoy)
            .bind(u.max_visitas_nuevo)
            .bind(u.max_visitas_recurrente)
            .bind(i32::try_from(u.dias_inactividad).unwrap_or(i32::MAX))
            .fetch_one(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(IndicadoresClientesDto {
            registrados,
            por_segmento: ConteoSegmentosDto {
                sin_visitas,
                nuevo,
                recurrente,
                frecuente,
                inactivo,
            },
            dias_inactividad: u.dias_inactividad,
        })
    }
}

#[derive(sqlx::FromRow)]
struct FilaSolicitud {
    id: i64,
    cliente_id: Option<i64>,
    tipo: String,
    descripcion: String,
    recibida_en: DateTime<Utc>,
    respondida_en: Option<DateTime<Utc>>,
    respondida_por: Option<i64>,
    respuesta: Option<String>,
}

impl From<FilaSolicitud> for SolicitudGuardada {
    fn from(f: FilaSolicitud) -> Self {
        SolicitudGuardada {
            id: f.id,
            cliente: f.cliente_id.map(IdCliente),
            tipo: f.tipo,
            descripcion: f.descripcion,
            recibida_en: f.recibida_en,
            respondida_en: f.respondida_en,
            respondida_por: f.respondida_por,
            respuesta: f.respuesta,
        }
    }
}

const COLUMNAS_SOLICITUD: &str = "id, cliente_id, tipo::text AS tipo, descripcion, recibida_en, respondida_en, \
                                  respondida_por, respuesta";

#[async_trait]
impl RepositorioTitular<UnidadDeTrabajo> for RepositorioClientesPg {
    async fn anonimizar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdCliente,
        ahora: DateTime<Utc>,
    ) -> Result<bool, ErrorPersistencia> {
        let filas = sqlx::query(
            "UPDATE clientes.cliente
             SET celular = NULL, nombre = NULL, correo = NULL, fecha_nacimiento = NULL, preferencias = NULL,
                 recordatorios_activos = false, anonimizado_en = $2
             WHERE id = $1 AND anonimizado_en IS NULL",
        )
        .bind(id.0)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?
        .rows_affected();
        // La prueba de la autorización se conserva, pero queda revocada.
        sqlx::query(
            "UPDATE clientes.autorizacion SET revocada_en = $2 WHERE cliente_id = $1 AND revocada_en IS NULL",
        )
        .bind(id.0)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas == 1)
    }

    async fn solicitudes(
        &self,
        uow: &mut UnidadDeTrabajo,
        solo_abiertas: bool,
        pagina: SolicitudPagina,
    ) -> Result<(Vec<SolicitudGuardada>, i64), ErrorPersistencia> {
        let filas: Vec<FilaSolicitud> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNAS_SOLICITUD} FROM clientes.solicitud_titular
             WHERE (NOT $1 OR respondida_en IS NULL)
             ORDER BY recibida_en DESC, id DESC LIMIT $2 OFFSET $3"
        )))
        .bind(solo_abiertas)
        .bind(pagina.limite())
        .bind(pagina.desplazamiento())
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM clientes.solicitud_titular WHERE (NOT $1 OR respondida_en IS NULL)",
        )
        .bind(solo_abiertas)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok((filas.into_iter().map(SolicitudGuardada::from).collect(), total))
    }

    async fn crear_solicitud(
        &self,
        uow: &mut UnidadDeTrabajo,
        cliente: Option<IdCliente>,
        tipo: &str,
        descripcion: &str,
        ahora: DateTime<Utc>,
    ) -> Result<SolicitudGuardada, ErrorPersistencia> {
        let fila: FilaSolicitud = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "INSERT INTO clientes.solicitud_titular (cliente_id, tipo, descripcion, recibida_en)
             VALUES ($1, $2::clientes.tipo_solicitud_titular, $3, $4)
             RETURNING {COLUMNAS_SOLICITUD}"
        )))
        .bind(cliente.map(|c| c.0))
        .bind(tipo)
        .bind(descripcion)
        .bind(ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.into())
    }

    async fn solicitud(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
    ) -> Result<Option<SolicitudGuardada>, ErrorPersistencia> {
        let fila: Option<FilaSolicitud> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNAS_SOLICITUD} FROM clientes.solicitud_titular WHERE id = $1 FOR UPDATE"
        )))
        .bind(id)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(SolicitudGuardada::from))
    }

    async fn responder_solicitud(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i64,
        usuario: i64,
        respuesta: &str,
        ahora: DateTime<Utc>,
    ) -> Result<Option<SolicitudGuardada>, ErrorPersistencia> {
        let fila: Option<FilaSolicitud> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE clientes.solicitud_titular SET respondida_en = $2, respondida_por = $3, respuesta = $4
             WHERE id = $1 AND respondida_en IS NULL
             RETURNING {COLUMNAS_SOLICITUD}"
        )))
        .bind(id)
        .bind(ahora)
        .bind(usuario)
        .bind(respuesta)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(SolicitudGuardada::from))
    }
}
