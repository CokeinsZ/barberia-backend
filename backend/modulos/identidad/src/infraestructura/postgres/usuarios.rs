use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::identificadores::IdUsuario;
use nucleo::{Correo, ErrorPersistencia};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::dto::{AdministradorDto, CuentaPersonalDto};
use crate::aplicacion::puertos::RepositorioUsuarios;
use crate::dominio::{NombrePersona, Rol, Usuario};

/// Columnas de una cuenta con el nombre del administrador (si lo es).
pub(super) const COLUMNAS_USUARIO: &str = "u.id AS usuario_id, u.correo::text AS correo, u.rol::text AS rol, \
     a.nombre AS nombre_administrador, u.hash_contrasena, u.activo, u.intentos_fallidos, u.bloqueado_hasta";

#[derive(sqlx::FromRow)]
pub(super) struct FilaUsuario {
    usuario_id: i64,
    correo: String,
    rol: String,
    nombre_administrador: Option<String>,
    hash_contrasena: Option<String>,
    activo: bool,
    intentos_fallidos: i16,
    bloqueado_hasta: Option<DateTime<Utc>>,
}

impl FilaUsuario {
    pub(super) fn a_dominio(self) -> Result<Usuario, ErrorPersistencia> {
        Ok(Usuario {
            id: IdUsuario(self.usuario_id),
            correo: Correo::nuevo(&self.correo)
                .map_err(|e| ErrorPersistencia::Otro(format!("correo guardado inválido: {e}")))?,
            rol: Rol::desde_str(&self.rol)
                .ok_or_else(|| ErrorPersistencia::Otro(format!("rol desconocido: {}", self.rol)))?,
            nombre_administrador: self.nombre_administrador,
            hash_contrasena: self.hash_contrasena,
            activo: self.activo,
            intentos_fallidos: self.intentos_fallidos,
            bloqueado_hasta: self.bloqueado_hasta,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioUsuariosPg;

#[derive(sqlx::FromRow)]
struct FilaAdministrador {
    id: i64,
    correo: String,
    nombre: String,
    activo: bool,
    pendiente_activacion: bool,
    ultimo_acceso_en: Option<DateTime<Utc>>,
}

#[async_trait]
impl RepositorioUsuarios<UnidadDeTrabajo> for RepositorioUsuariosPg {
    async fn por_correo(
        &self,
        uow: &mut UnidadDeTrabajo,
        correo: &Correo,
    ) -> Result<Option<Usuario>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT {COLUMNAS_USUARIO}
             FROM identidad.usuario u
             LEFT JOIN identidad.administrador a ON a.usuario_id = u.id
             WHERE u.correo = $1::citext
             FOR UPDATE OF u"
        );
        // La consulta se arma solo con constantes de este archivo.
        sqlx::query_as::<_, FilaUsuario>(sqlx::AssertSqlSafe(consulta))
            .bind(correo.como_str())
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?
            .map(FilaUsuario::a_dominio)
            .transpose()
    }

    async fn por_id(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdUsuario,
    ) -> Result<Option<Usuario>, ErrorPersistencia> {
        let consulta = format!(
            "SELECT {COLUMNAS_USUARIO}
             FROM identidad.usuario u
             LEFT JOIN identidad.administrador a ON a.usuario_id = u.id
             WHERE u.id = $1"
        );
        sqlx::query_as::<_, FilaUsuario>(sqlx::AssertSqlSafe(consulta))
            .bind(id.0)
            .fetch_optional(uow.conexion())
            .await
            .map_err(a_error_persistencia)?
            .map(FilaUsuario::a_dominio)
            .transpose()
    }

    async fn guardar_acceso(
        &self,
        uow: &mut UnidadDeTrabajo,
        usuario: &Usuario,
        acceso: Option<DateTime<Utc>>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE identidad.usuario
             SET intentos_fallidos = $2, bloqueado_hasta = $3, ultimo_acceso_en = coalesce($4, ultimo_acceso_en)
             WHERE id = $1",
        )
        .bind(usuario.id.0)
        .bind(usuario.intentos_fallidos)
        .bind(usuario.bloqueado_hasta)
        .bind(acceso)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn guardar_contrasena(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdUsuario,
        hash: &str,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE identidad.usuario
             SET hash_contrasena = $2, intentos_fallidos = 0, bloqueado_hasta = NULL
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(hash)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn crear_administrador(
        &self,
        uow: &mut UnidadDeTrabajo,
        correo: &Correo,
        nombre: &NombrePersona,
        ahora: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "WITH nuevo AS (
                 INSERT INTO identidad.usuario (correo, rol, creado_en, actualizado_en)
                 VALUES ($1::citext, 'administrador', $3, $3)
                 RETURNING id
             )
             INSERT INTO identidad.administrador (usuario_id, nombre)
             SELECT id, $2 FROM nuevo
             RETURNING usuario_id",
        )
        .bind(correo.como_str())
        .bind(nombre.como_str())
        .bind(ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdUsuario(id))
    }

    async fn listar_administradores(
        &self,
        uow: &mut UnidadDeTrabajo,
    ) -> Result<Vec<AdministradorDto>, ErrorPersistencia> {
        let filas: Vec<FilaAdministrador> = sqlx::query_as(
            "SELECT u.id, u.correo::text AS correo, a.nombre, u.activo,
                    (u.hash_contrasena IS NULL) AS pendiente_activacion, u.ultimo_acceso_en
             FROM identidad.usuario u
             JOIN identidad.administrador a ON a.usuario_id = u.id
             ORDER BY u.activo DESC, a.nombre, u.id",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas
            .into_iter()
            .map(|f| AdministradorDto {
                id: IdUsuario(f.id),
                correo: f.correo,
                nombre: f.nombre,
                activo: f.activo,
                pendiente_activacion: f.pendiente_activacion,
                ultimo_acceso_en: f.ultimo_acceso_en,
            })
            .collect())
    }

    async fn administradores_activos(
        &self,
        uow: &mut UnidadDeTrabajo,
    ) -> Result<Vec<IdUsuario>, ErrorPersistencia> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM identidad.usuario WHERE rol = 'administrador' AND activo ORDER BY id FOR UPDATE",
        )
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(ids.into_iter().map(IdUsuario).collect())
    }

    async fn cambiar_activo(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdUsuario,
        activo: bool,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE identidad.usuario SET activo = $2 WHERE id = $1")
            .bind(id.0)
            .bind(activo)
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn crear_barbero(
        &self,
        uow: &mut UnidadDeTrabajo,
        correo: &Correo,
        activo: bool,
        ahora: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO identidad.usuario (correo, rol, activo, creado_en, actualizado_en)
             VALUES ($1::citext, 'barbero', $2, $3, $3)
             RETURNING id",
        )
        .bind(correo.como_str())
        .bind(activo)
        .bind(ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdUsuario(id))
    }

    async fn cambiar_correo(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdUsuario,
        correo: &Correo,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query("UPDATE identidad.usuario SET correo = $2::citext WHERE id = $1")
            .bind(id.0)
            .bind(correo.como_str())
            .execute(uow.conexion())
            .await
            .map_err(a_error_persistencia)?;
        Ok(())
    }

    async fn resumen_cuenta(
        &self,
        uow: &mut UnidadDeTrabajo,
        id: IdUsuario,
    ) -> Result<Option<CuentaPersonalDto>, ErrorPersistencia> {
        let fila: Option<(i64, String, bool, bool, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, correo::text, activo, hash_contrasena IS NULL, ultimo_acceso_en
             FROM identidad.usuario WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(
            fila.map(|(id, correo, activo, pendiente, ultimo)| CuentaPersonalDto {
                usuario: IdUsuario(id),
                correo,
                acceso_panel: activo,
                pendiente_activacion: pendiente,
                ultimo_acceso_en: ultimo,
            }),
        )
    }
}
