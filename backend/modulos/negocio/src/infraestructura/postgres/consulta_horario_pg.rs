use async_trait::async_trait;
use chrono::NaiveTime;
use nucleo::ErrorPersistencia;
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::ConsultaHorarioAtencion;

/// Lee la vista publicada `barberos.v_horario_atencion` (contrato de lectura del módulo
/// `barberos`): `negocio` no conoce las tablas de los barberos.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ConsultaHorarioAtencionPg;

#[async_trait]
impl ConsultaHorarioAtencion<UnidadDeTrabajo> for ConsultaHorarioAtencionPg {
    async fn por_dia(
        &self,
        uow: &mut UnidadDeTrabajo,
    ) -> Result<Vec<(i16, NaiveTime, NaiveTime)>, ErrorPersistencia> {
        sqlx::query_as("SELECT dia_semana, abre, cierra FROM barberos.v_horario_atencion ORDER BY dia_semana")
            .fetch_all(uow.conexion())
            .await
            .map_err(a_error_persistencia)
    }
}
