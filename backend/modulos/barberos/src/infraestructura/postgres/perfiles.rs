//! Implementación del puerto `identidad::api::PerfilesBarberos` (inversión de dependencias:
//! `barberos` depende de `identidad`, nunca al revés).

use std::sync::Arc;

use async_trait::async_trait;
use identidad::api::{PerfilBarbero, PerfilesBarberos};
use nucleo::identificadores::{IdBarbero, IdUsuario};
use nucleo::{ErrorPersistencia, Reloj};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

#[derive(Debug)]
pub(crate) struct PerfilesBarberosPg {
    reloj: Arc<dyn Reloj>,
}

impl PerfilesBarberosPg {
    pub(crate) fn nuevo(reloj: Arc<dyn Reloj>) -> Self {
        Self { reloj }
    }
}

#[derive(sqlx::FromRow)]
struct FilaPerfil {
    id: i64,
    nombre: String,
    puede_acceder: bool,
}

#[async_trait]
impl PerfilesBarberos<UnidadDeTrabajo> for PerfilesBarberosPg {
    async fn por_usuario(
        &self,
        uow: &mut UnidadDeTrabajo,
        usuario: IdUsuario,
    ) -> Result<Option<PerfilBarbero>, ErrorPersistencia> {
        // El estado del día sale de `barberos.estados_en`, la misma regla que usa la agenda:
        // retirado («inactivo») o suspendido hoy no entra al panel (RF-AUT-08).
        let fila: Option<FilaPerfil> = sqlx::query_as(
            "SELECT b.id, b.nombres || ' ' || b.apellidos AS nombre,
                    e.estado NOT IN ('inactivo', 'suspendido') AS puede_acceder
             FROM barberos.barbero b
             JOIN barberos.estados_en($2) e ON e.barbero_id = b.id
             WHERE b.usuario_id = $1",
        )
        .bind(usuario.0)
        .bind(self.reloj.hoy())
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|f| PerfilBarbero {
            barbero: IdBarbero(f.id),
            nombre: f.nombre,
            puede_acceder: f.puede_acceder,
        }))
    }
}
