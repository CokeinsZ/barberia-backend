//! Fachada pública del módulo `negocio`: lo único que los demás módulos pueden usar.

use async_trait::async_trait;
use chrono::NaiveDate;

pub use crate::aplicacion::dto::{DatosNegocioDto, DiaEspecialDto, TipoDiaEspecialDto};
pub use crate::aplicacion::errores::ErrorNegocio;

/// Calendario del negocio. El módulo `agenda` lo usa para no ofrecer horas en días cerrados
/// y para aplicar el horario de festivo (RN-03).
#[async_trait]
pub trait CalendarioNegocio: Send + Sync + std::fmt::Debug {
    /// Días especiales entre dos fechas (incluidas), ordenados por fecha.
    async fn dias_especiales(
        &self,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio>;
}
