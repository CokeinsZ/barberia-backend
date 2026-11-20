use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::{ErrorPersistencia, SolicitudPagina};

use super::dto::RegistroAuditoriaDto;

/// Filtros ya traducidos a instantes.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConsultaRegistros {
    pub(crate) entidad: Option<String>,
    pub(crate) entidad_id: Option<String>,
    pub(crate) desde: Option<DateTime<Utc>>,
    pub(crate) hasta: Option<DateTime<Utc>>,
    pub(crate) pagina: SolicitudPagina,
}

#[async_trait]
pub(crate) trait RepositorioRegistros<C: Send>: Send + Sync + std::fmt::Debug {
    /// Registros más recientes primero, con el total que cumple los filtros.
    async fn listar(
        &self,
        contexto: &mut C,
        consulta: &ConsultaRegistros,
    ) -> Result<(Vec<RegistroAuditoriaDto>, i64), ErrorPersistencia>;
}
