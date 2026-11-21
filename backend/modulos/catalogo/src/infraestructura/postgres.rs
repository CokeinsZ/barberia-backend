//! Repositorio del esquema `catalogo`.

use async_trait::async_trait;
use nucleo::identificadores::{IdCategoria, IdServicio};
use nucleo::{Dinero, ErrorPersistencia};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};
use sqlx::types::Json;

use crate::aplicacion::dto::{CategoriaDto, ComponenteDto};
use crate::aplicacion::puertos::{RepositorioServicios, ServicioGuardado};
use crate::dominio::{DatosServicio, TipoServicio};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioServiciosPg;

/// Columnas comunes del listado y de la consulta por identificador.
const SELECCION: &str = "
    SELECT s.id, s.tipo::text AS tipo, s.categoria_id, c.nombre AS categoria, s.nombre, s.descripcion,
           s.precio, s.duracion_min, s.agendable_en_linea, s.visible_en_landing, s.activo, s.destacado,
           s.etiqueta, s.orden,
           coalesce((SELECT json_agg(json_build_object('id', k.id, 'nombre', k.nombre) ORDER BY k.nombre)
                     FROM catalogo.componente_combo cc
                     JOIN catalogo.servicio k ON k.id = cc.servicio_id
                     WHERE cc.combo_id = s.id), '[]'::json) AS componentes
    FROM catalogo.servicio s
    JOIN catalogo.categoria c ON c.id = s.categoria_id";

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    tipo: String,
    categoria_id: i16,
    categoria: String,
    nombre: String,
    descripcion: String,
    precio: i64,
    duracion_min: Option<i16>,
    agendable_en_linea: bool,
    visible_en_landing: bool,
    activo: bool,
    destacado: bool,
    etiqueta: Option<String>,
    orden: i16,
    componentes: Json<Vec<ComponenteDto>>,
}

impl TryFrom<Fila> for ServicioGuardado {
    type Error = ErrorPersistencia;

    fn try_from(f: Fila) -> Result<Self, Self::Error> {
        let invalido = |que: &str| ErrorPersistencia::Otro(format!("servicio {} con {que} inválido", f.id));
        Ok(ServicioGuardado {
            id: IdServicio(f.id),
            datos: DatosServicio {
                tipo: TipoServicio::desde_codigo(&f.tipo).ok_or_else(|| invalido("tipo"))?,
                categoria: IdCategoria(i64::from(f.categoria_id)),
                nombre: f.nombre.clone(),
                descripcion: f.descripcion.clone(),
                precio: Dinero::pesos(f.precio).map_err(|_| invalido("precio"))?,
                duracion_min: f.duracion_min,
                agendable_en_linea: f.agendable_en_linea,
                visible_en_landing: f.visible_en_landing,
                destacado: f.destacado,
                etiqueta: f.etiqueta.clone(),
                orden: f.orden,
            },
            activo: f.activo,
            categoria_nombre: f.categoria.clone(),
            componentes: f.componentes.0,
        })
    }
}

fn categoria_smallint(datos: &DatosServicio) -> i16 {
    i16::try_from(datos.categoria.0).unwrap_or(i16::MAX)
}

#[async_trait]
impl RepositorioServicios<UnidadDeTrabajo> for RepositorioServiciosPg {
    async fn categorias(&self, uow: &mut UnidadDeTrabajo) -> Result<Vec<CategoriaDto>, ErrorPersistencia> {
        let filas: Vec<(i16, String, i16)> =
            sqlx::query_as("SELECT id, nombre, orden FROM catalogo.categoria ORDER BY orden, id")
                .fetch_all(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|(id, nombre, orden)| CategoriaDto {
                id: IdCategoria(i64::from(id)),
                nombre,
                orden,
            })
            .collect())
    }

    async fn listar(
        &self,
        uow: &mut UnidadDeTrabajo,
        incluir_inactivos: bool,
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia> {
        let consulta = format!("{SELECCION} WHERE ($1 OR s.activo) ORDER BY c.orden, s.orden, s.nombre");
        // La consulta se arma solo con constantes de este archivo.
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(incluir_inactivos)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        filas.into_iter().map(ServicioGuardado::try_from).collect()
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[IdServicio],
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        let consulta = format!("{SELECCION} WHERE s.id = ANY($1) ORDER BY s.id");
        let filas: Vec<Fila> = sqlx::query_as(sqlx::AssertSqlSafe(consulta))
            .bind(&ids)
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        filas.into_iter().map(ServicioGuardado::try_from).collect()
    }

    async fn crear(
        &self,
        uow: &mut UnidadDeTrabajo,
        d: &DatosServicio,
    ) -> Result<IdServicio, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO catalogo.servicio (tipo, categoria_id, nombre, descripcion, precio, duracion_min,
                                            agendable_en_linea, visible_en_landing, destacado, etiqueta, orden)
             VALUES ($1::catalogo.tipo_servicio, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING id",
        )
        .bind(d.tipo.codigo())
        .bind(categoria_smallint(d))
        .bind(&d.nombre)
        .bind(&d.descripcion)
        .bind(d.precio.valor())
        .bind(d.duracion_min)
        .bind(d.agendable_en_linea)
        .bind(d.visible_en_landing)
        .bind(d.destacado)
        .bind(&d.etiqueta)
        .bind(d.orden)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdServicio(id))
    }

    async fn actualizar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdServicio,
        d: &DatosServicio,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE catalogo.servicio
             SET categoria_id = $2, nombre = $3, descripcion = $4, precio = $5, duracion_min = $6,
                 agendable_en_linea = $7, visible_en_landing = $8, destacado = $9, etiqueta = $10, orden = $11
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(categoria_smallint(d))
        .bind(&d.nombre)
        .bind(&d.descripcion)
        .bind(d.precio.valor())
        .bind(d.duracion_min)
        .bind(d.agendable_en_linea)
        .bind(d.visible_en_landing)
        .bind(d.destacado)
        .bind(&d.etiqueta)
        .bind(d.orden)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn cambiar_estado(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdServicio,
        activo: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE catalogo.servicio SET activo = $2 WHERE id = $1")
            .bind(id.0)
            .bind(activo)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn reemplazar_componentes(
        &self,
        uow: &mut UnidadDeTrabajo,
        combo: IdServicio,
        componentes: &[IdServicio],
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("DELETE FROM catalogo.componente_combo WHERE combo_id = $1")
            .bind(combo.0)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        let ids: Vec<i64> = componentes.iter().map(|c| c.0).collect();
        sqlx::query(
            "INSERT INTO catalogo.componente_combo (combo_id, servicio_id)
             SELECT $1, unnest($2::bigint[])",
        )
        .bind(combo.0)
        .bind(&ids)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}
