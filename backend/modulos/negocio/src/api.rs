//! Fachada pública del módulo `negocio`: lo único que los demás módulos pueden usar.

use async_trait::async_trait;
use chrono::NaiveDate;
use nucleo::Porcentaje;

pub use crate::aplicacion::dto::{
    DatosNegocioDto, DiaEspecialDto, FestivosPrecargadosDto, HorarioDiaDto, PoliticaDto, TipoDiaEspecialDto,
};
pub use crate::aplicacion::errores::ErrorNegocio;

/// Calendario del negocio. El módulo `agenda` lo usa para no ofrecer horas en días cerrados
/// y para aplicar el horario de festivo (RN-03). Lee dentro del contexto transaccional `C` de
/// quien llama.
#[async_trait]
pub trait CalendarioNegocio<C: Send>: Send + Sync + std::fmt::Debug {
    /// Días especiales entre dos fechas (incluidas), ordenados por fecha.
    async fn dias_especiales(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio>;
}

/// Parámetros del negocio que necesitan otros módulos. Lee dentro del contexto de quien llama.
#[async_trait]
pub trait ParametrosNegocio<C: Send>: Send + Sync + std::fmt::Debug {
    /// Comisión por defecto para un barbero nuevo (RF-BAR-04).
    async fn comision_por_defecto(&self, cx: &mut C) -> Result<Porcentaje, ErrorNegocio>;
}

/// Datos del negocio que se muestran en el sitio público (sin NIT ni comisión).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NegocioPublicoDto {
    pub nombre_comercial: String,
    pub direccion: Option<String>,
    pub zona_referencia: Option<String>,
    /// Celular de 10 dígitos, sin indicativo.
    pub whatsapp: Option<String>,
    pub instagram: Option<String>,
}

/// Lo que la landing y el agendamiento necesitan del negocio. Sin política de actor: son datos
/// públicos. Lee dentro del contexto de quien llama.
#[async_trait]
pub trait DatosPublicos<C: Send>: Send + Sync + std::fmt::Debug {
    async fn negocio(&self, cx: &mut C) -> Result<NegocioPublicoDto, ErrorNegocio>;
    /// RF-CFG-03.
    async fn horario_atencion(&self, cx: &mut C) -> Result<Vec<HorarioDiaDto>, ErrorNegocio>;
    /// La versión vigente de cada tipo de política (RNF-LEG-01).
    async fn politicas_vigentes(&self, cx: &mut C) -> Result<Vec<PoliticaDto>, ErrorNegocio>;
}
