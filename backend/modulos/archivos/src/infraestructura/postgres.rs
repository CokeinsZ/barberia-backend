use async_trait::async_trait;
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdArchivo;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{NuevoRegistro, RegistroArchivo, RepositorioArchivos};
use crate::dominio::{ClaveArchivo, TipoArchivo, Visibilidad};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioArchivosPg;

#[derive(sqlx::FromRow)]
struct Fila {
    id: i64,
    clave: String,
    nombre_original: String,
    tipo_mime: String,
    tamano_bytes: i32,
    visibilidad: String,
}

impl TryFrom<Fila> for RegistroArchivo {
    type Error = ErrorPersistencia;

    fn try_from(f: Fila) -> Result<Self, Self::Error> {
        let invalido = |que: &str| ErrorPersistencia::Otro(format!("archivo {} con {que} inválido", f.id));
        Ok(RegistroArchivo {
            id: IdArchivo(f.id),
            clave: ClaveArchivo::validar(&f.clave).ok_or_else(|| invalido("clave"))?,
            nombre_original: f.nombre_original.clone(),
            tipo: TipoArchivo::desde_mime(&f.tipo_mime).ok_or_else(|| invalido("tipo"))?,
            tamano_bytes: usize::try_from(f.tamano_bytes).unwrap_or_default(),
            visibilidad: Visibilidad::desde_str(&f.visibilidad).ok_or_else(|| invalido("visibilidad"))?,
        })
    }
}

#[async_trait]
impl RepositorioArchivos<UnidadDeTrabajo> for RepositorioArchivosPg {
    async fn insertar(
        &self,
        uow: &mut UnidadDeTrabajo,
        nuevo: &NuevoRegistro,
    ) -> Result<IdArchivo, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO archivos.archivo
                 (clave, nombre_original, tipo_mime, tamano_bytes, sha256, visibilidad, subido_por, subido_en)
             VALUES ($1, $2, $3, $4, $5, $6::archivos.visibilidad, $7, $8)
             RETURNING id",
        )
        .bind(nuevo.clave.como_str())
        .bind(&nuevo.nombre_original)
        .bind(nuevo.tipo.mime())
        .bind(i32::try_from(nuevo.tamano_bytes).unwrap_or(i32::MAX))
        .bind(&nuevo.sha256[..])
        .bind(nuevo.visibilidad.como_str())
        .bind(nuevo.subido_por.map(|u| u.0))
        .bind(nuevo.ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdArchivo(id))
    }

    async fn obtener(
        &self,
        uow: &mut UnidadDeTrabajo,
        ids: &[IdArchivo],
    ) -> Result<Vec<RegistroArchivo>, ErrorPersistencia> {
        let ids: Vec<i64> = ids.iter().map(|i| i.0).collect();
        let filas: Vec<Fila> = sqlx::query_as(
            "SELECT id, clave, nombre_original, tipo_mime, tamano_bytes, visibilidad::text AS visibilidad
             FROM archivos.archivo WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        filas.into_iter().map(RegistroArchivo::try_from).collect()
    }

    async fn eliminar(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdArchivo,
    ) -> Result<Option<ClaveArchivo>, ErrorPersistencia> {
        let clave: Option<String> =
            sqlx::query_scalar("DELETE FROM archivos.archivo WHERE id = $1 RETURNING clave")
                .bind(id.0)
                .fetch_optional(uow.conexion())
                .await
                .map_err(a_error_persistencia)?;
        Ok(clave.and_then(|c| ClaveArchivo::validar(&c)))
    }
}
