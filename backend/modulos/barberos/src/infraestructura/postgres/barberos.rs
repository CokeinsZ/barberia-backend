use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use nucleo::identificadores::{IdArchivo, IdBarbero, IdServicio, IdUsuario};
use nucleo::{ErrorPersistencia, Porcentaje};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use sqlx::types::Json;

use crate::api::{AusenciaAgenda, BarberoAgenda, FranjaAgenda};
use crate::aplicacion::dto::ServicioOfrecidoDto;
use crate::aplicacion::puertos::{BarberoGuardado, RepositorioBarberos};
use crate::dominio::{DatosBarbero, EstadoBarbero, Franja, Horario, Retiro};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioBarberosPg;

/// Ficha con el estado del día (`barberos.estados_en`, RN-24) y los nombres de sus servicios
/// (vista publicada `catalogo.v_servicio`).
const SELECCION: &str = "
    SELECT b.id, b.usuario_id, b.tipo_documento::text AS tipo_documento, b.numero_documento, b.nombres, b.apellidos,
           b.fecha_nacimiento, b.celular::text AS celular, b.especialidad, b.anios_experiencia, b.instagram, b.biografia,
           b.foto_archivo_id, b.fecha_ingreso, b.tipo_vinculacion::text AS tipo_vinculacion, b.fin_contrato,
           (b.porcentaje_comision * 100)::int4 AS comision_centesimas, b.periodo_liquidacion::text AS periodo_liquidacion,
           b.mostrar_en_landing, b.recibe_citas_en_linea, b.orden, b.fecha_retiro, b.motivo_retiro::text AS motivo_retiro,
           e.estado::text AS estado, e.ausencia_desde, e.ausencia_hasta,
           coalesce((SELECT json_agg(json_build_object('id', s.id, 'nombre', s.nombre) ORDER BY s.nombre)
                     FROM barberos.servicio_ofrecido so
                     JOIN catalogo.v_servicio s ON s.id = so.servicio_id
                     WHERE so.barbero_id = b.id), '[]'::json) AS servicios
    FROM barberos.barbero b
    JOIN barberos.estados_en($1) e ON e.barbero_id = b.id";

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    usuario_id: i64,
    tipo_documento: String,
    numero_documento: String,
    nombres: String,
    apellidos: String,
    fecha_nacimiento: NaiveDate,
    celular: String,
    especialidad: String,
    anios_experiencia: Option<i16>,
    instagram: Option<String>,
    biografia: Option<String>,
    foto_archivo_id: Option<i64>,
    fecha_ingreso: NaiveDate,
    tipo_vinculacion: String,
    fin_contrato: Option<NaiveDate>,
    comision_centesimas: i32,
    periodo_liquidacion: String,
    mostrar_en_landing: bool,
    recibe_citas_en_linea: bool,
    orden: i16,
    fecha_retiro: Option<NaiveDate>,
    motivo_retiro: Option<String>,
    estado: String,
    ausencia_desde: Option<NaiveDate>,
    ausencia_hasta: Option<NaiveDate>,
    servicios: Json<Vec<ServicioOfrecidoDto>>,
}

impl TryFrom<Fila> for BarberoGuardado {
    type Error = ErrorPersistencia;

    fn try_from(f: Fila) -> Result<Self, Self::Error> {
        let invalido = |que: &str| ErrorPersistencia::Otro(format!("barbero {} con {que} inválido", f.id));
        Ok(BarberoGuardado {
            id: IdBarbero(f.id),
            usuario: IdUsuario(f.usuario_id),
            tipo_documento: f.tipo_documento.clone(),
            numero_documento: f.numero_documento.clone(),
            nombres: f.nombres.clone(),
            apellidos: f.apellidos.clone(),
            fecha_nacimiento: f.fecha_nacimiento,
            celular: f.celular.clone(),
            especialidad: f.especialidad.clone(),
            anios_experiencia: f.anios_experiencia,
            instagram: f.instagram.clone(),
            biografia: f.biografia.clone(),
            foto: f.foto_archivo_id.map(IdArchivo),
            fecha_ingreso: f.fecha_ingreso,
            tipo_vinculacion: f.tipo_vinculacion.clone(),
            fin_contrato: f.fin_contrato,
            porcentaje_comision: u16::try_from(f.comision_centesimas)
                .ok()
                .and_then(|c| Porcentaje::desde_centesimas(c).ok())
                .ok_or_else(|| invalido("porcentaje de comisión"))?,
            periodo_liquidacion: f.periodo_liquidacion.clone(),
            mostrar_en_landing: f.mostrar_en_landing,
            recibe_citas_en_linea: f.recibe_citas_en_linea,
            orden: f.orden,
            fecha_retiro: f.fecha_retiro,
            motivo_retiro: f.motivo_retiro.clone(),
            estado: EstadoBarbero::desde_codigo(&f.estado).ok_or_else(|| invalido("estado"))?,
            estado_desde: f.ausencia_desde,
            estado_hasta: f.ausencia_hasta,
            servicios: f.servicios.0,
        })
    }
}

fn centesimas(datos: &DatosBarbero) -> i32 {
    i32::from(datos.porcentaje_comision.centesimas())
}

#[async_trait]
impl RepositorioBarberos<UnidadDeTrabajo> for RepositorioBarberosPg {
    async fn equipo_agenda(
        &self,
        uow: &mut UnidadDeTrabajo,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(BarberoAgenda, Option<IdArchivo>)>, ErrorPersistencia> {
        #[derive(sqlx::FromRow)]
        struct FilaEquipo {
            id: i64,
            nombres: String,
            apellidos: String,
            especialidad: String,
            anios_experiencia: Option<i16>,
            instagram: Option<String>,
            biografia: Option<String>,
            foto_archivo_id: Option<i64>,
            orden: i16,
            mostrar_en_landing: bool,
            recibe_citas_en_linea: bool,
            fecha_retiro: Option<NaiveDate>,
            servicios: Vec<i64>,
            franjas: Json<Vec<(i16, String, String)>>,
        }
        let filas: Vec<FilaEquipo> = sqlx::query_as(
            "SELECT b.id, b.nombres, b.apellidos, b.especialidad, b.anios_experiencia, b.instagram, b.biografia,
                    b.foto_archivo_id, b.orden, b.mostrar_en_landing, b.recibe_citas_en_linea, b.fecha_retiro,
                    array(SELECT so.servicio_id FROM barberos.servicio_ofrecido so WHERE so.barbero_id = b.id ORDER BY 1) AS servicios,
                    coalesce((SELECT json_agg(json_build_array(f.dia_semana, f.hora_inicio::text, f.hora_fin::text)
                                              ORDER BY f.dia_semana, f.hora_inicio)
                              FROM barberos.franja_horaria f WHERE f.barbero_id = b.id), '[]'::json) AS franjas
             FROM barberos.barbero b
             WHERE b.fecha_retiro IS NULL OR b.fecha_retiro > $1
             ORDER BY b.orden, b.id",
        )
        .bind(desde)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        let ausencias: Vec<(i64, String, NaiveDate, NaiveDate)> = sqlx::query_as(
            "SELECT barbero_id, tipo::text, desde, hasta FROM barberos.ausencia
             WHERE anulada_en IS NULL AND desde <= $2 AND hasta >= $1",
        )
        .bind(desde)
        .bind(hasta)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        // Del inicio de `desde` al final de `hasta` en Bogotá (con un día de margen por la zona).
        let bloqueos: Vec<(i64, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> =
            sqlx::query_as(
                "SELECT barbero_id, inicio, fin FROM barberos.bloqueo
             WHERE inicio < ($2::date + 2)::timestamp AT TIME ZONE 'UTC'
               AND fin > ($1::date - 1)::timestamp AT TIME ZONE 'UTC'
             ORDER BY inicio",
            )
            .bind(desde)
            .bind(hasta)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        let hora = |texto: &str| NaiveTime::parse_from_str(texto, "%H:%M:%S").unwrap_or(NaiveTime::MIN);
        Ok(filas
            .into_iter()
            .map(|f| {
                let barbero = BarberoAgenda {
                    id: IdBarbero(f.id),
                    nombre: format!("{} {}", f.nombres, f.apellidos),
                    nombres: f.nombres,
                    especialidad: f.especialidad,
                    anios_experiencia: f.anios_experiencia,
                    instagram: f.instagram,
                    biografia: f.biografia,
                    foto_url: None,
                    orden: f.orden,
                    mostrar_en_landing: f.mostrar_en_landing,
                    recibe_citas_en_linea: f.recibe_citas_en_linea,
                    fecha_retiro: f.fecha_retiro,
                    servicios: f.servicios.into_iter().map(IdServicio).collect(),
                    franjas: f
                        .franjas
                        .0
                        .iter()
                        .map(|(dia, inicio, fin)| FranjaAgenda {
                            dia_semana: u8::try_from(*dia).unwrap_or_default(),
                            inicio: hora(inicio),
                            fin: hora(fin),
                        })
                        .collect(),
                    ausencias: ausencias
                        .iter()
                        .filter(|a| a.0 == f.id)
                        .map(|a| AusenciaAgenda {
                            tipo: a.1.clone(),
                            desde: a.2,
                            hasta: a.3,
                        })
                        .collect(),
                    bloqueos: bloqueos
                        .iter()
                        .filter(|b| b.0 == f.id)
                        .map(|b| (b.1, b.2))
                        .collect(),
                };
                (barbero, f.foto_archivo_id.map(IdArchivo))
            })
            .collect())
    }

    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        fecha: NaiveDate,
    ) -> Result<Vec<BarberoGuardado>, ErrorPersistencia> {
        let consulta = format!("{SELECCION} ORDER BY (e.estado = 'inactivo'), b.orden, b.id");
        // La consulta se arma solo con constantes de este archivo.
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(fecha)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        filas.into_iter().map(BarberoGuardado::try_from).collect()
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        fecha: NaiveDate,
    ) -> Result<Option<BarberoGuardado>, ErrorPersistencia> {
        let consulta = format!("{SELECCION} WHERE b.id = $2");
        let fila: Option<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(fecha)
            .bind(id.0)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        fila.map(BarberoGuardado::try_from).transpose()
    }

    async fn nombres(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[IdBarbero],
    ) -> Result<std::collections::HashMap<IdBarbero, String>, ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        let filas: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, nombres || ' ' || apellidos FROM barberos.barbero WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(id, nombre)| (IdBarbero(id), nombre))
            .collect())
    }

    async fn bloquear(&self, uow: &mut UnidadDeTrabajo, id: IdBarbero) -> Result<bool, ErrorPersistencia> {
        let fila: Option<i64> =
            sqlx::query_scalar("SELECT id FROM barberos.barbero WHERE id = $1 FOR UPDATE")
                .bind(id.0)
                .fetch_optional(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(fila.is_some())
    }

    async fn crear(
        &self,
        uow: &mut UnidadDeTrabajo,
        usuario: IdUsuario,
        d: &DatosBarbero,
        mostrar_en_landing: bool,
        recibe_citas_en_linea: bool,
    ) -> Result<IdBarbero, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO barberos.barbero
                 (usuario_id, tipo_documento, numero_documento, nombres, apellidos, fecha_nacimiento, celular, especialidad,
                  anios_experiencia, instagram, biografia, fecha_ingreso, tipo_vinculacion, fin_contrato, porcentaje_comision,
                  periodo_liquidacion, mostrar_en_landing, recibe_citas_en_linea, orden)
             VALUES ($1, $2::barberos.tipo_documento, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::barberos.tipo_vinculacion,
                     $14, $15::numeric / 100, $16::barberos.periodo_liquidacion, $17, $18,
                     (SELECT coalesce(max(orden), 0) + 1 FROM barberos.barbero))
             RETURNING id",
        )
        .bind(usuario.0)
        .bind(d.tipo_documento.codigo())
        .bind(&d.numero_documento)
        .bind(&d.nombres)
        .bind(&d.apellidos)
        .bind(d.fecha_nacimiento)
        .bind(d.celular.digitos())
        .bind(&d.especialidad)
        .bind(d.anios_experiencia)
        .bind(&d.instagram)
        .bind(&d.biografia)
        .bind(d.fecha_ingreso)
        .bind(d.tipo_vinculacion.codigo())
        .bind(d.fin_contrato)
        .bind(centesimas(d))
        .bind(d.periodo_liquidacion.codigo())
        .bind(mostrar_en_landing)
        .bind(recibe_citas_en_linea)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdBarbero(id))
    }

    async fn actualizar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        d: &DatosBarbero,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE barberos.barbero
             SET tipo_documento = $2::barberos.tipo_documento, numero_documento = $3, nombres = $4, apellidos = $5,
                 fecha_nacimiento = $6, celular = $7, especialidad = $8, anios_experiencia = $9, instagram = $10,
                 biografia = $11, fecha_ingreso = $12, tipo_vinculacion = $13::barberos.tipo_vinculacion,
                 fin_contrato = $14, porcentaje_comision = $15::numeric / 100,
                 periodo_liquidacion = $16::barberos.periodo_liquidacion
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(d.tipo_documento.codigo())
        .bind(&d.numero_documento)
        .bind(&d.nombres)
        .bind(&d.apellidos)
        .bind(d.fecha_nacimiento)
        .bind(d.celular.digitos())
        .bind(&d.especialidad)
        .bind(d.anios_experiencia)
        .bind(&d.instagram)
        .bind(&d.biografia)
        .bind(d.fecha_ingreso)
        .bind(d.tipo_vinculacion.codigo())
        .bind(d.fin_contrato)
        .bind(centesimas(d))
        .bind(d.periodo_liquidacion.codigo())
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn cambiar_visibilidad(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        landing: bool,
        en_linea: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE barberos.barbero SET mostrar_en_landing = $2, recibe_citas_en_linea = $3 WHERE id = $1",
        )
        .bind(id.0)
        .bind(landing)
        .bind(en_linea)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn vigentes(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<IdBarbero>, ErrorPersistencia> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM barberos.barbero WHERE fecha_retiro IS NULL ORDER BY orden, id FOR UPDATE",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(ids.into_iter().map(IdBarbero).collect())
    }

    async fn reordenar(&self, uow: &mut UnidadDeTrabajo, ids: &[IdBarbero]) -> Result<(), ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        sqlx::query(
            "UPDATE barberos.barbero b SET orden = o.posicion
             FROM unnest($1::bigint[]) WITH ORDINALITY AS o(id, posicion)
             WHERE b.id = o.id",
        )
        .bind(&ids)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn retirar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        retiro: &Retiro,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE barberos.barbero SET fecha_retiro = $2, motivo_retiro = $3::barberos.motivo_retiro WHERE id = $1")
            .bind(id.0)
            .bind(retiro.fecha)
            .bind(retiro.motivo.codigo())
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn cambiar_foto(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        foto: IdArchivo,
    ) -> Result<Option<IdArchivo>, ErrorPersistencia> {
        let anterior: Option<i64> =
            sqlx::query_scalar("SELECT foto_archivo_id FROM barberos.barbero WHERE id = $1")
                .bind(id.0)
                .fetch_one(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        sqlx::query("UPDATE barberos.barbero SET foto_archivo_id = $2 WHERE id = $1")
            .bind(id.0)
            .bind(foto.0)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(anterior.map(IdArchivo))
    }

    async fn franjas(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
    ) -> Result<Vec<Franja>, ErrorPersistencia> {
        let filas: Vec<(i16, NaiveTime, NaiveTime)> = sqlx::query_as(
            "SELECT dia_semana, hora_inicio, hora_fin FROM barberos.franja_horaria
             WHERE barbero_id = $1 ORDER BY dia_semana, hora_inicio",
        )
        .bind(id.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(dia, inicio, fin)| Franja {
                dia_semana: u8::try_from(dia).unwrap_or_default(),
                inicio,
                fin,
            })
            .collect())
    }

    async fn reemplazar_franjas(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        horario: &Horario,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("DELETE FROM barberos.franja_horaria WHERE barbero_id = $1")
            .bind(id.0)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        let dias: Vec<i16> = horario
            .franjas()
            .iter()
            .map(|f| i16::from(f.dia_semana))
            .collect();
        let inicios: Vec<NaiveTime> = horario.franjas().iter().map(|f| f.inicio).collect();
        let fines: Vec<NaiveTime> = horario.franjas().iter().map(|f| f.fin).collect();
        sqlx::query(
            "INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin)
             SELECT $1, dia, inicio, fin FROM unnest($2::smallint[], $3::time[], $4::time[]) AS f(dia, inicio, fin)",
        )
        .bind(id.0)
        .bind(&dias)
        .bind(&inicios)
        .bind(&fines)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn reemplazar_servicios(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdBarbero,
        ids: &[IdServicio],
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("DELETE FROM barberos.servicio_ofrecido WHERE barbero_id = $1")
            .bind(id.0)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        sqlx::query("INSERT INTO barberos.servicio_ofrecido (barbero_id, servicio_id) SELECT $1, unnest($2::bigint[])")
            .bind(id.0)
            .bind(&ids)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }
}
