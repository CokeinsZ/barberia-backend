//! Lecturas de los reportes sobre las **vistas publicadas** de los demás módulos (regla 4 del
//! plan): `ventas.v_venta`, `ventas.v_venta_linea`, `ventas.v_venta_pago`, `agenda.v_cita`,
//! `catalogo.v_servicio`, `barberos.v_barbero` y `clientes.v_cliente_resumen`. Nunca tablas.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdBarbero;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::dto::{
    BarberoDto, ClienteDestacadoDto, ClienteReactivarDto, ClientesPeriodoDto, MetodoPagoDto,
    ServicioVendidoDto, VentaDetalleDto,
};
use crate::aplicacion::puertos::{CitaVista, FilaBarbero, LecturasReportes, Sumas};

#[derive(Debug, Default)]
pub(crate) struct LecturasPg;

#[derive(sqlx::FromRow)]
struct FilaSumas {
    ventas: i64,
    servicios: i64,
    subtotal: i64,
    descuentos: i64,
    ventas_netas: i64,
    propinas: i64,
    total_cobrado: i64,
    comisiones: i64,
}

impl From<FilaSumas> for Sumas {
    fn from(f: FilaSumas) -> Self {
        Sumas {
            ventas: f.ventas,
            servicios: f.servicios,
            subtotal: f.subtotal,
            descuentos: f.descuentos,
            ventas_netas: f.ventas_netas,
            propinas: f.propinas,
            total_cobrado: f.total_cobrado,
            comisiones: f.comisiones,
        }
    }
}

/// Sumas de `v_venta` (sum de bigint da numeric: se vuelve a bigint).
const SUMAS: &str = "count(*)::bigint AS ventas,
     coalesce(sum(v.subtotal), 0)::bigint AS subtotal,
     coalesce(sum(v.descuento_valor), 0)::bigint AS descuentos,
     coalesce(sum(v.venta_neta), 0)::bigint AS ventas_netas,
     coalesce(sum(v.propina), 0)::bigint AS propinas,
     coalesce(sum(v.total_cobrado), 0)::bigint AS total_cobrado,
     coalesce(sum(v.comision), 0)::bigint AS comisiones";

#[derive(sqlx::FromRow)]
struct FilaCita {
    id: i64,
    inicio: DateTime<Utc>,
    fin: DateTime<Utc>,
    estado: String,
    cliente: Option<String>,
    servicio: String,
    barbero_id: i64,
    barbero: String,
}

#[async_trait]
impl LecturasReportes<UnidadDeTrabajo> for LecturasPg {
    async fn sumas(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<Sumas, ErrorPersistencia> {
        let fila: FilaSumas = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMAS},
                    (SELECT count(*) FROM ventas.v_venta_linea l
                     WHERE l.fecha_operacion BETWEEN $1 AND $2 AND ($3::bigint IS NULL OR l.barbero_id = $3))::bigint AS servicios
             FROM ventas.v_venta v
             WHERE v.fecha_operacion BETWEEN $1 AND $2 AND ($3::bigint IS NULL OR v.barbero_id = $3)"
        )))
        .bind(desde)
        .bind(hasta)
        .bind(barbero.map(|b| b.0))
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.into())
    }

    async fn sumas_hasta(
        &self,
        uow: &mut UnidadDeTrabajo,
        fecha: NaiveDate,
        hasta: DateTime<Utc>,
    ) -> Result<Sumas, ErrorPersistencia> {
        let fila: FilaSumas = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMAS},
                    (SELECT count(*) FROM ventas.v_venta_linea l
                     WHERE l.fecha_operacion = $1 AND l.registrada_en <= $2)::bigint AS servicios
             FROM ventas.v_venta v
             WHERE v.fecha_operacion = $1 AND v.registrada_en <= $2"
        )))
        .bind(fecha)
        .bind(hasta)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.into())
    }

    async fn netas_por_dia(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64)>, ErrorPersistencia> {
        sqlx::query_as(
            "SELECT fecha_operacion, sum(venta_neta)::bigint FROM ventas.v_venta
             WHERE fecha_operacion BETWEEN $1 AND $2 GROUP BY 1 ORDER BY 1",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn por_servicio(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<Vec<ServicioVendidoDto>, ErrorPersistencia> {
        let filas: Vec<(i64, String, String, i64, i64)> = sqlx::query_as(
            "SELECT l.servicio_id, s.nombre, s.categoria, count(*)::bigint, sum(l.precio)::bigint AS ingresos
             FROM ventas.v_venta_linea l
             JOIN catalogo.v_servicio s ON s.id = l.servicio_id
             WHERE l.fecha_operacion BETWEEN $1 AND $2 AND ($3::bigint IS NULL OR l.barbero_id = $3)
             GROUP BY l.servicio_id, s.nombre, s.categoria
             ORDER BY ingresos DESC, count(*) DESC, s.nombre",
        )
        .bind(desde)
        .bind(hasta)
        .bind(barbero.map(|b| b.0))
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(
                |(servicio_id, nombre, categoria, unidades, ingresos)| ServicioVendidoDto {
                    servicio_id,
                    nombre,
                    categoria,
                    unidades,
                    ingresos,
                },
            )
            .collect())
    }

    async fn por_metodo(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<MetodoPagoDto>, ErrorPersistencia> {
        let filas: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT metodo, metodo_nombre, sum(monto)::bigint AS total FROM ventas.v_venta_pago
             WHERE fecha_operacion BETWEEN $1 AND $2
             GROUP BY metodo, metodo_nombre ORDER BY total DESC, metodo",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(metodo, nombre, total)| MetodoPagoDto {
                metodo,
                nombre,
                total,
            })
            .collect())
    }

    async fn por_barbero(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<FilaBarbero>, ErrorPersistencia> {
        #[derive(sqlx::FromRow)]
        struct Fila {
            barbero_id: i64,
            nombre: String,
            #[sqlx(flatten)]
            sumas: FilaSumas,
        }
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT v.barbero_id, b.nombre, {SUMAS},
                    (SELECT count(*) FROM ventas.v_venta_linea l
                     WHERE l.barbero_id = v.barbero_id AND l.fecha_operacion BETWEEN $1 AND $2)::bigint AS servicios
             FROM ventas.v_venta v
             JOIN barberos.v_barbero b ON b.id = v.barbero_id
             WHERE v.fecha_operacion BETWEEN $1 AND $2
             GROUP BY v.barbero_id, b.nombre
             ORDER BY ventas_netas DESC, b.nombre"
        )))
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|f| FilaBarbero {
                barbero_id: f.barbero_id,
                nombre: f.nombre,
                sumas: f.sumas.into(),
            })
            .collect())
    }

    async fn barbero(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
    ) -> Result<Option<BarberoDto>, ErrorPersistencia> {
        #[allow(clippy::type_complexity)]
        let fila: Option<(i64, String, String, String, String, NaiveDate, i32, Option<NaiveDate>)> = sqlx::query_as(
            "SELECT id, nombre, tipo_documento::text, numero_documento, tipo_vinculacion::text, fecha_ingreso,
                    (porcentaje_comision * 100)::int4, fecha_retiro
             FROM barberos.v_barbero WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(
            |(
                id,
                nombre,
                tipo_documento,
                numero_documento,
                tipo_vinculacion,
                fecha_ingreso,
                centesimas,
                fecha_retiro,
            )| BarberoDto {
                id,
                nombre,
                tipo_documento,
                numero_documento,
                tipo_vinculacion,
                fecha_ingreso,
                porcentaje_comision: f64::from(centesimas) / 100.0,
                fecha_retiro,
            },
        ))
    }

    async fn barberos_del_periodo(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<IdBarbero>, ErrorPersistencia> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT b.id FROM barberos.v_barbero b
             WHERE (b.fecha_ingreso <= $2 AND (b.fecha_retiro IS NULL OR b.fecha_retiro >= $1))
                OR EXISTS (SELECT 1 FROM ventas.v_venta v WHERE v.barbero_id = b.id AND v.fecha_operacion BETWEEN $1 AND $2)
             ORDER BY b.orden, b.id",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(ids.into_iter().map(IdBarbero).collect())
    }

    async fn ventas_de_barbero(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: IdBarbero,
    ) -> Result<Vec<VentaDetalleDto>, ErrorPersistencia> {
        #[allow(clippy::type_complexity)]
        let filas: Vec<(
            i64,
            i64,
            DateTime<Utc>,
            Option<i64>,
            Option<String>,
            String,
            i64,
            i64,
            i64,
        )> = sqlx::query_as(
            "SELECT v.id, v.numero_ticket, v.registrada_en, v.cliente_id, r.nombre,
                    coalesce((SELECT string_agg(s.nombre, ', ' ORDER BY l.posicion)
                              FROM ventas.v_venta_linea l JOIN catalogo.v_servicio s ON s.id = l.servicio_id
                              WHERE l.venta_id = v.id), ''),
                    v.venta_neta::bigint, v.propina::bigint, v.comision
             FROM ventas.v_venta v
             LEFT JOIN clientes.v_cliente_resumen r ON r.id = v.cliente_id
             WHERE v.barbero_id = $3 AND v.fecha_operacion BETWEEN $1 AND $2
             ORDER BY v.registrada_en, v.id",
        )
        .bind(desde)
        .bind(hasta)
        .bind(barbero.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(
                |(
                    venta_id,
                    numero_ticket,
                    registrada_en,
                    cliente_id,
                    nombre,
                    servicios,
                    ventas_netas,
                    propina,
                    comision,
                )| {
                    VentaDetalleDto {
                        venta_id,
                        numero_ticket,
                        registrada_en: registrada_en.with_timezone(&nucleo::ZONA_HORARIA).fixed_offset(),
                        cliente: match (cliente_id, nombre) {
                            (None, _) => "Cliente ocasional".to_owned(),
                            (Some(_), Some(n)) => n,
                            (Some(_), None) => "Cliente anonimizado".to_owned(),
                        },
                        servicios,
                        ventas_netas,
                        propina,
                        comision,
                    }
                },
            )
            .collect())
    }

    async fn clientes_distintos(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: IdBarbero,
    ) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar(
            "SELECT count(DISTINCT cliente_id) FROM ventas.v_venta
             WHERE barbero_id = $3 AND fecha_operacion BETWEEN $1 AND $2 AND cliente_id IS NOT NULL",
        )
        .bind(desde)
        .bind(hasta)
        .bind(barbero.0)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn clientes_del_periodo(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<ClientesPeriodoDto, ErrorPersistencia> {
        // Retorno (RN-21): clientes del periodo que, en alguna de sus visitas del periodo,
        // volvían en menos de 60 días desde la anterior (el mismo día no cuenta).
        let (atendidos, ocasionales, nuevos, con_retorno): (i64, i64, i64, i64) = sqlx::query_as(
            "WITH del_periodo AS (
                 SELECT DISTINCT cliente_id FROM ventas.v_venta
                 WHERE fecha_operacion BETWEEN $1 AND $2 AND cliente_id IS NOT NULL
             ),
             historial AS (
                 SELECT v.cliente_id, v.fecha_operacion,
                        lag(v.fecha_operacion) OVER (PARTITION BY v.cliente_id ORDER BY v.fecha_operacion, v.id) AS anterior,
                        min(v.fecha_operacion) OVER (PARTITION BY v.cliente_id) AS primera
                 FROM ventas.v_venta v
                 WHERE v.cliente_id IN (SELECT cliente_id FROM del_periodo) AND v.fecha_operacion <= $2
             )
             SELECT (SELECT count(*) FROM del_periodo)::bigint,
                    (SELECT count(*) FROM ventas.v_venta
                     WHERE fecha_operacion BETWEEN $1 AND $2 AND cliente_id IS NULL)::bigint,
                    (SELECT count(DISTINCT cliente_id) FROM historial WHERE primera BETWEEN $1 AND $2)::bigint,
                    (SELECT count(DISTINCT cliente_id) FROM historial
                     WHERE fecha_operacion BETWEEN $1 AND $2 AND anterior IS NOT NULL
                       AND fecha_operacion - anterior BETWEEN 1 AND 59)::bigint",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(ClientesPeriodoDto {
            atendidos,
            ocasionales,
            nuevos,
            con_retorno,
            ..Default::default()
        })
    }

    async fn inasistencias(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
    ) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar(
            "SELECT count(*) FROM agenda.v_cita WHERE estado = 'no_asistio' AND inicio >= $1 AND inicio < $2",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn mas_visitas(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
        limite: i64,
    ) -> Result<Vec<ClienteDestacadoDto>, ErrorPersistencia> {
        let filas: Vec<(i64, Option<String>, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT v.cliente_id, r.nombre, r.celular::text, count(*)::bigint AS visitas, sum(v.venta_neta)::bigint AS netas
             FROM ventas.v_venta v
             LEFT JOIN clientes.v_cliente_resumen r ON r.id = v.cliente_id
             WHERE v.fecha_operacion BETWEEN $1 AND $2 AND v.cliente_id IS NOT NULL
             GROUP BY v.cliente_id, r.nombre, r.celular
             ORDER BY visitas DESC, netas DESC, v.cliente_id
             LIMIT $3",
        )
        .bind(desde)
        .bind(hasta)
        .bind(limite)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(
                |(cliente_id, nombre, celular, visitas, ventas_netas)| ClienteDestacadoDto {
                    cliente_id,
                    nombre: nombre.unwrap_or_else(|| "Cliente anonimizado".to_owned()),
                    celular,
                    visitas,
                    ventas_netas,
                },
            )
            .collect())
    }

    async fn por_reactivar(
        &self,
        uow: &mut UnidadDeTrabajo,
        ultima_antes_de: NaiveDate,
        hoy: NaiveDate,
        limite: i64,
    ) -> Result<(Vec<ClienteReactivarDto>, i64), ErrorPersistencia> {
        #[allow(clippy::type_complexity)]
        // El último servicio se busca solo para los listados, no para todos los inactivos.
        let filas: Vec<(i64, String, String, NaiveDate, Option<String>, i64)> = sqlx::query_as(
            "WITH inactivos AS (
                 SELECT r.id, r.nombre, r.celular::text AS celular, r.ultima_visita, count(*) OVER () AS total
                 FROM clientes.v_cliente_resumen r
                 WHERE r.ultima_visita < $1
                 ORDER BY r.ultima_visita DESC, r.id
                 LIMIT $2
             )
             SELECT i.id, i.nombre, i.celular, i.ultima_visita,
                    (SELECT s.nombre FROM ventas.v_venta_linea l JOIN catalogo.v_servicio s ON s.id = l.servicio_id
                     WHERE l.cliente_id = i.id ORDER BY l.registrada_en DESC, l.posicion LIMIT 1),
                    i.total
             FROM inactivos i
             ORDER BY i.ultima_visita DESC, i.id",
        )
        .bind(ultima_antes_de)
        .bind(limite)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let total = filas.first().map_or(0, |f| f.5);
        Ok((
            filas
                .into_iter()
                .map(
                    |(cliente_id, nombre, celular, ultima_visita, ultimo_servicio, _)| ClienteReactivarDto {
                        cliente_id,
                        nombre,
                        celular,
                        dias_sin_volver: (hoy - ultima_visita).num_days(),
                        ultima_visita,
                        ultimo_servicio,
                    },
                )
                .collect(),
            total,
        ))
    }

    async fn citas(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
        estados: &[&str],
    ) -> Result<Vec<CitaVista>, ErrorPersistencia> {
        let estados: Vec<String> = estados.iter().map(|e| (*e).to_owned()).collect();
        let filas: Vec<FilaCita> = sqlx::query_as(
            "SELECT c.id, c.inicio, c.fin, c.estado::text AS estado, r.nombre AS cliente, s.nombre AS servicio,
                    c.barbero_id, b.nombre AS barbero
             FROM agenda.v_cita c
             JOIN catalogo.v_servicio s ON s.id = c.servicio_id
             JOIN barberos.v_barbero b ON b.id = c.barbero_id
             LEFT JOIN clientes.v_cliente_resumen r ON r.id = c.cliente_id
             WHERE c.inicio >= $1 AND c.inicio < $2 AND (cardinality($3::text[]) = 0 OR c.estado::text = ANY($3))
             ORDER BY c.inicio, c.id",
        )
        .bind(desde)
        .bind(hasta)
        .bind(&estados)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|f| CitaVista {
                id: f.id,
                inicio: f.inicio,
                fin: f.fin,
                estado: f.estado,
                cliente: f.cliente,
                servicio: f.servicio,
                barbero_id: f.barbero_id,
                barbero: f.barbero,
            })
            .collect())
    }

    async fn cumpleanos(
        &self,
        uow: &mut UnidadDeTrabajo,
        dias: &[(u32, u32)],
    ) -> Result<Vec<(i64, String, String, NaiveDate)>, ErrorPersistencia> {
        // Mismas expresiones que el índice `cliente_cumpleanos_idx`.
        let meses: Vec<i32> = dias.iter().map(|(m, _)| i32::try_from(*m).unwrap_or(0)).collect();
        let dias_del_mes: Vec<i32> = dias.iter().map(|(_, d)| i32::try_from(*d).unwrap_or(0)).collect();
        sqlx::query_as(
            "SELECT id, nombre, celular::text, fecha_nacimiento FROM clientes.v_cliente_resumen
             WHERE fecha_nacimiento IS NOT NULL
               AND (extract(month FROM fecha_nacimiento), extract(day FROM fecha_nacimiento)) IN
                   (SELECT m, d FROM unnest($1::int4[], $2::int4[]) AS t(m, d))
             ORDER BY nombre",
        )
        .bind(&meses)
        .bind(&dias_del_mes)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn atendidos_por_mes(
        &self,
        uow: &mut UnidadDeTrabajo,
        anio: i32,
    ) -> Result<Vec<(u32, i64)>, ErrorPersistencia> {
        let filas: Vec<(i32, i64)> = sqlx::query_as(
            "SELECT extract(month FROM fecha_operacion)::int4, count(*)::bigint FROM ventas.v_venta
             WHERE fecha_operacion >= make_date($1, 1, 1) AND fecha_operacion < make_date($1 + 1, 1, 1)
             GROUP BY 1 ORDER BY 1",
        )
        .bind(anio)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(mes, cantidad)| (u32::try_from(mes).unwrap_or(1), cantidad))
            .collect())
    }

    async fn ventas_por_hora(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(u8, u32, i64)>, ErrorPersistencia> {
        let filas: Vec<(i32, i32, i64)> = sqlx::query_as(
            "SELECT extract(isodow FROM registrada_en AT TIME ZONE 'America/Bogota')::int4,
                    extract(hour FROM registrada_en AT TIME ZONE 'America/Bogota')::int4,
                    count(*)::bigint
             FROM ventas.v_venta WHERE fecha_operacion BETWEEN $1 AND $2
             GROUP BY 1, 2 ORDER BY 1, 2",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(dia, hora, cantidad)| {
                (
                    u8::try_from(dia).unwrap_or(1),
                    u32::try_from(hora).unwrap_or(0),
                    cantidad,
                )
            })
            .collect())
    }

    async fn clientes_inactivos(
        &self,
        uow: &mut UnidadDeTrabajo,
        ultima_antes_de: NaiveDate,
    ) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar("SELECT count(*) FROM clientes.v_cliente_resumen WHERE ultima_visita < $1")
            .bind(ultima_antes_de)
            .fetch_one(uow.conexion())
            .await
            .map_err(a_error_persistencia)
    }
}
