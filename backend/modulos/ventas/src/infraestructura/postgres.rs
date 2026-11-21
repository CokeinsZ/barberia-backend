//! Esquema `ventas` en PostgreSQL. El cuadre de cada venta lo verifica además la base al
//! confirmar (triggers diferidos `verificar_cuadre`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio, IdUsuario, IdVenta};
use nucleo::{Dinero, ErrorPersistencia, Porcentaje, SolicitudPagina};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{
    CierreGuardado, Descuento, FiltroVentas, MetodoPago, Motivo, NuevaVenta, NuevoCierre, RepositorioVentas,
    VentaGuardada,
};
use crate::dominio::TipoDescuento;

#[derive(Debug, Default)]
pub(crate) struct RepositorioVentasPg;

fn dinero(valor: i64) -> Result<Dinero, ErrorPersistencia> {
    Dinero::pesos(valor).map_err(|e| ErrorPersistencia::Otro(e.to_string()))
}

fn porcentaje(centesimas: i32) -> Result<Porcentaje, ErrorPersistencia> {
    u16::try_from(centesimas)
        .ok()
        .and_then(|c| Porcentaje::desde_centesimas(c).ok())
        .ok_or_else(|| ErrorPersistencia::Otro(format!("porcentaje inválido: {centesimas}")))
}

#[derive(sqlx::FromRow)]
struct FilaVenta {
    id: i64,
    numero_ticket: i64,
    registrada_en: DateTime<Utc>,
    fecha_operacion: NaiveDate,
    registrada_por: i64,
    barbero_id: i64,
    cliente_id: Option<i64>,
    cita_id: Option<i64>,
    descuento_id: Option<i16>,
    motivo_descuento_id: Option<i16>,
    descuento_valor: i64,
    propina: i64,
    comision_centesimas: i32,
    estado: String,
    anulada_en: Option<DateTime<Utc>>,
    anulada_por: Option<i64>,
    motivo_anulacion: Option<String>,
}

const COLUMNAS: &str = "id, numero_ticket, registrada_en, fecha_operacion, registrada_por, barbero_id, cliente_id, cita_id,
     descuento_id, motivo_descuento_id, descuento_valor::bigint AS descuento_valor, propina::bigint AS propina,
     (porcentaje_comision * 100)::int4 AS comision_centesimas, estado::text AS estado, anulada_en, anulada_por,
     motivo_anulacion";

impl RepositorioVentasPg {
    /// Completa las ventas con sus líneas y pagos (dos consultas para toda la lista).
    async fn completar(
        &self,
        uow: &mut UnidadDeTrabajo,
        filas: Vec<FilaVenta>,
    ) -> Result<Vec<VentaGuardada>, ErrorPersistencia> {
        let ids: Vec<i64> = filas.iter().map(|f| f.id).collect();
        let lineas: Vec<(i64, i16, i64, i64)> = sqlx::query_as(
            "SELECT venta_id, posicion, servicio_id, precio::bigint FROM ventas.venta_linea
             WHERE venta_id = ANY($1) ORDER BY venta_id, posicion",
        )
        .bind(&ids)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let pagos: Vec<(i64, i16, i64)> = sqlx::query_as(
            "SELECT venta_id, metodo_pago_id, monto::bigint FROM ventas.venta_pago
             WHERE venta_id = ANY($1) ORDER BY venta_id, metodo_pago_id",
        )
        .bind(&ids)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let mut lineas_por: HashMap<i64, Vec<(i16, IdServicio, Dinero)>> = HashMap::new();
        for (venta, posicion, servicio, precio) in lineas {
            lineas_por
                .entry(venta)
                .or_default()
                .push((posicion, IdServicio(servicio), dinero(precio)?));
        }
        let mut pagos_por: HashMap<i64, Vec<(i16, Dinero)>> = HashMap::new();
        for (venta, metodo, monto) in pagos {
            pagos_por.entry(venta).or_default().push((metodo, dinero(monto)?));
        }
        filas
            .into_iter()
            .map(|f| {
                Ok(VentaGuardada {
                    id: IdVenta(f.id),
                    numero_ticket: f.numero_ticket,
                    registrada_en: f.registrada_en,
                    fecha_operacion: f.fecha_operacion,
                    registrada_por: IdUsuario(f.registrada_por),
                    barbero: IdBarbero(f.barbero_id),
                    cliente: f.cliente_id.map(IdCliente),
                    cita: f.cita_id.map(IdCita),
                    descuento: match (f.descuento_id, f.motivo_descuento_id) {
                        (Some(d), Some(m)) => Some((d, m, dinero(f.descuento_valor)?)),
                        _ => None,
                    },
                    propina: dinero(f.propina)?,
                    porcentaje_comision: porcentaje(f.comision_centesimas)?,
                    estado: f.estado,
                    anulacion: match (f.anulada_en, f.anulada_por, f.motivo_anulacion) {
                        (Some(en), Some(por), Some(motivo)) => Some((en, IdUsuario(por), motivo)),
                        _ => None,
                    },
                    lineas: lineas_por.remove(&f.id).unwrap_or_default(),
                    pagos: pagos_por.remove(&f.id).unwrap_or_default(),
                })
            })
            .collect()
    }
}

#[async_trait]
impl RepositorioVentas<UnidadDeTrabajo> for RepositorioVentasPg {
    async fn metodos_pago(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<MetodoPago>, ErrorPersistencia> {
        let filas: Vec<(i16, String, String, bool, i16)> = sqlx::query_as(
            "SELECT id, codigo, nombre, activo, orden FROM ventas.metodo_pago ORDER BY orden, id",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(id, codigo, nombre, activo, orden)| MetodoPago {
                id,
                codigo,
                nombre,
                activo,
                orden,
            })
            .collect())
    }

    async fn cambiar_metodo(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: i16,
        activo: bool,
    ) -> Result<Option<MetodoPago>, ErrorPersistencia> {
        let fila: Option<(i16, String, String, bool, i16)> = sqlx::query_as(
            "UPDATE ventas.metodo_pago SET activo = $2 WHERE id = $1 RETURNING id, codigo, nombre, activo, orden",
        )
        .bind(id)
        .bind(activo)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|(id, codigo, nombre, activo, orden)| MetodoPago {
            id,
            codigo,
            nombre,
            activo,
            orden,
        }))
    }

    async fn descuentos(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<Descuento>, ErrorPersistencia> {
        let filas: Vec<(i16, String, String, i64, bool, bool)> = sqlx::query_as(
            "SELECT id, nombre, tipo::text, (valor * 100)::bigint, permitido_a_barberos, activo
             FROM ventas.descuento ORDER BY orden, id",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        filas
            .into_iter()
            .map(|(id, nombre, tipo, centesimas, permitido_a_barberos, activo)| {
                let tipo = if tipo == "porcentaje" {
                    TipoDescuento::Porcentaje(porcentaje(i32::try_from(centesimas).unwrap_or(i32::MAX))?)
                } else {
                    TipoDescuento::ValorFijo(dinero(centesimas / 100)?)
                };
                Ok(Descuento {
                    id,
                    nombre,
                    tipo,
                    permitido_a_barberos,
                    activo,
                })
            })
            .collect()
    }

    async fn motivos(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<Motivo>, ErrorPersistencia> {
        let filas: Vec<(i16, String, bool)> =
            sqlx::query_as("SELECT id, nombre, activo FROM ventas.motivo_descuento ORDER BY orden, id")
                .fetch_all(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(id, nombre, activo)| Motivo { id, nombre, activo })
            .collect())
    }

    async fn siguiente_ticket(&self, uow: &mut UnidadDeTrabajo) -> Result<i64, ErrorPersistencia> {
        sqlx::query_scalar(
            "UPDATE ventas.contador_ticket SET ultimo = ultimo + 1 WHERE id = 1 RETURNING ultimo",
        )
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn insertar(
        &self,
        uow: &mut UnidadDeTrabajo,
        v: &NuevaVenta,
    ) -> Result<IdVenta, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO ventas.venta (numero_ticket, registrada_en, registrada_por, barbero_id, cliente_id, cita_id,
                                       descuento_id, motivo_descuento_id, descuento_valor, propina, porcentaje_comision)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::numeric / 100)
             RETURNING id",
        )
        .bind(v.numero_ticket)
        .bind(v.registrada_en)
        .bind(v.registrada_por.0)
        .bind(v.barbero.0)
        .bind(v.cliente.map(|c| c.0))
        .bind(v.cita.map(|c| c.0))
        .bind(v.descuento.map(|d| d.0))
        .bind(v.descuento.map(|d| d.1))
        .bind(v.descuento.map_or(0, |d| d.2.valor()))
        .bind(v.propina.valor())
        .bind(i32::from(v.porcentaje_comision.centesimas()))
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        for (posicion, (servicio, precio)) in v.lineas.iter().enumerate() {
            sqlx::query("INSERT INTO ventas.venta_linea (venta_id, posicion, servicio_id, precio) VALUES ($1, $2, $3, $4)")
                .bind(id)
                .bind(i16::try_from(posicion + 1).unwrap_or(i16::MAX))
                .bind(servicio.0)
                .bind(precio.valor())
                .execute(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        }
        for (metodo, monto) in &v.pagos {
            sqlx::query(
                "INSERT INTO ventas.venta_pago (venta_id, metodo_pago_id, monto) VALUES ($1, $2, $3)",
            )
            .bind(id)
            .bind(metodo)
            .bind(monto.valor())
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        }
        Ok(IdVenta(id))
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdVenta,
        bloquear: bool,
    ) -> Result<Option<VentaGuardada>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT {COLUMNAS} FROM ventas.venta WHERE id = $1{}",
            if bloquear { " FOR UPDATE" } else { "" }
        );
        let fila: Option<FilaVenta> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(id.0)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        match fila {
            Some(f) => Ok(self.completar(uow, vec![f]).await?.pop()),
            None => Ok(None),
        }
    }

    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        f: &FiltroVentas,
        pagina: SolicitudPagina,
    ) -> Result<(Vec<VentaGuardada>, i64), ErrorPersistencia> {
        const CONDICION: &str = "($1::date IS NULL OR fecha_operacion >= $1)
             AND ($2::date IS NULL OR fecha_operacion <= $2)
             AND ($3::bigint IS NULL OR barbero_id = $3)
             AND ($4::bigint IS NULL OR cliente_id = $4)
             AND (NOT $5 OR estado = 'registrada')";
        let filas: Vec<FilaVenta> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNAS} FROM ventas.venta WHERE {CONDICION}
             ORDER BY registrada_en DESC, id DESC LIMIT $6 OFFSET $7"
        )))
        .bind(f.desde)
        .bind(f.hasta)
        .bind(f.barbero.map(|b| b.0))
        .bind(f.cliente.map(|c| c.0))
        .bind(f.solo_registradas)
        .bind(pagina.limite())
        .bind(pagina.desplazamiento())
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM ventas.venta WHERE {CONDICION}"
        )))
        .bind(f.desde)
        .bind(f.hasta)
        .bind(f.barbero.map(|b| b.0))
        .bind(f.cliente.map(|c| c.0))
        .bind(f.solo_registradas)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok((self.completar(uow, filas).await?, total))
    }

    async fn anular(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdVenta,
        por: IdUsuario,
        motivo: &str,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE ventas.venta SET estado = 'anulada', anulada_en = $2, anulada_por = $3, motivo_anulacion = $4
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(ahora)
        .bind(por.0)
        .bind(motivo)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn ultima_visita_vigente(
        &self,
        uow: &mut UnidadDeTrabajo,
        cliente: IdCliente,
    ) -> Result<Option<NaiveDate>, ErrorPersistencia> {
        sqlx::query_scalar(
            "SELECT max(fecha_operacion) FROM ventas.venta WHERE cliente_id = $1 AND estado = 'registrada'",
        )
        .bind(cliente.0)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)
    }

    async fn cobrado_por_metodo(
        &self,
        uow: &mut UnidadDeTrabajo,
        fecha: NaiveDate,
    ) -> Result<Vec<(i16, Dinero)>, ErrorPersistencia> {
        let filas: Vec<(i16, i64)> = sqlx::query_as(
            "SELECT p.metodo_pago_id, sum(p.monto)::bigint
             FROM ventas.venta_pago p JOIN ventas.venta v ON v.id = p.venta_id
             WHERE v.fecha_operacion = $1 AND v.estado = 'registrada'
             GROUP BY p.metodo_pago_id ORDER BY p.metodo_pago_id",
        )
        .bind(fecha)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        filas
            .into_iter()
            .map(|(m, total)| Ok((m, dinero(total)?)))
            .collect()
    }

    async fn cierre(
        &self,
        uow: &mut UnidadDeTrabajo,
        fecha: NaiveDate,
    ) -> Result<Option<CierreGuardado>, ErrorPersistencia> {
        let fila: Option<(i64, NaiveDate, i64, DateTime<Utc>, Option<String>)> = sqlx::query_as(
            "SELECT id, fecha_operacion, cerrado_por, cerrado_en, observacion FROM ventas.cierre_caja
             WHERE fecha_operacion = $1",
        )
        .bind(fecha)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let Some((id, fecha, cerrado_por, cerrado_en, observacion)) = fila else {
            return Ok(None);
        };
        let detalle: Vec<(i16, i64, Option<i64>)> = sqlx::query_as(
            "SELECT metodo_pago_id, esperado::bigint, contado::bigint FROM ventas.cierre_caja_detalle
             WHERE cierre_id = $1 ORDER BY metodo_pago_id",
        )
        .bind(id)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(Some(CierreGuardado {
            id,
            fecha,
            cerrado_por: IdUsuario(cerrado_por),
            cerrado_en,
            observacion,
            detalle: detalle
                .into_iter()
                .map(|(m, esperado, contado)| Ok((m, dinero(esperado)?, contado.map(dinero).transpose()?)))
                .collect::<Result<_, ErrorPersistencia>>()?,
        }))
    }

    async fn crear_cierre(
        &self,
        uow: &mut UnidadDeTrabajo,
        c: &NuevoCierre,
    ) -> Result<i64, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO ventas.cierre_caja (fecha_operacion, cerrado_por, cerrado_en, observacion)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(c.fecha)
        .bind(c.cerrado_por.0)
        .bind(c.cerrado_en)
        .bind(&c.observacion)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        for (metodo, esperado, contado) in &c.detalle {
            sqlx::query(
                "INSERT INTO ventas.cierre_caja_detalle (cierre_id, metodo_pago_id, esperado, contado)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(metodo)
            .bind(esperado.valor())
            .bind(contado.map(Dinero::valor))
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        }
        Ok(id)
    }
}
