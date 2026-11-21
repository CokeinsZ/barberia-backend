//! Puertos: lo que los casos de uso necesitan del exterior, como traits pequeños y genéricos
//! en el contexto transaccional `C` (ADR 0005). Las implementaciones viven en
//! `infraestructura` (PostgreSQL, `C = UnidadDeTrabajo`) y en las pruebas (memoria, `C = ()`).

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nucleo::identificadores::IdDiaEspecial;
use nucleo::{ErrorPersistencia, Reloj, Transacciones};

use crate::dominio::entidades::{DatosNegocio, DiaEspecial, NuevaPolitica, TipoPolitica};

#[async_trait]
pub(crate) trait RepositorioNegocio<C: Send>: Send + Sync + fmt::Debug {
    async fn obtener(&self, cx: &mut C) -> Result<DatosNegocio, ErrorPersistencia>;
    async fn guardar(&self, cx: &mut C, datos: &DatosNegocio) -> Result<(), ErrorPersistencia>;
}

/// Día especial ya guardado, con su identificador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiaEspecialGuardado {
    pub(crate) id: IdDiaEspecial,
    pub(crate) dia: DiaEspecial,
}

#[async_trait]
pub(crate) trait RepositorioDiasEspeciales<C: Send>: Send + Sync + fmt::Debug {
    /// Días entre dos fechas (incluidas), ordenados por fecha.
    async fn listar(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialGuardado>, ErrorPersistencia>;
    /// Falla con `Duplicado { restriccion: "dia_especial_fecha_uk" }` si la fecha ya existe.
    async fn registrar(&self, cx: &mut C, dia: &DiaEspecial) -> Result<IdDiaEspecial, ErrorPersistencia>;
    /// Devuelve el día eliminado, o `None` si no existía.
    async fn eliminar(
        &self,
        cx: &mut C,
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia>;
    async fn obtener(
        &self,
        cx: &mut C,
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia>;
    /// Reemplaza el día. Falla con `Duplicado` si la nueva fecha ya está ocupada.
    async fn actualizar(
        &self,
        cx: &mut C,
        id: IdDiaEspecial,
        dia: &DiaEspecial,
    ) -> Result<(), ErrorPersistencia>;
    /// Registra solo si la fecha está libre (precarga idempotente). Devuelve el id si lo creó.
    async fn registrar_si_libre(
        &self,
        cx: &mut C,
        dia: &DiaEspecial,
    ) -> Result<Option<IdDiaEspecial>, ErrorPersistencia>;
}

/// Franja más temprana y más tardía de cada día de la semana (1 = lunes … 7 = domingo) entre
/// los barberos vigentes. Sale de la vista publicada `barberos.v_horario_atencion` (DA-10).
#[async_trait]
pub(crate) trait ConsultaHorarioAtencion<C: Send>: Send + Sync + fmt::Debug {
    async fn por_dia(&self, cx: &mut C) -> Result<Vec<(i16, NaiveTime, NaiveTime)>, ErrorPersistencia>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PoliticaGuardada {
    pub(crate) id: i64,
    pub(crate) tipo: TipoPolitica,
    pub(crate) version: String,
    pub(crate) url: String,
    pub(crate) vigente_desde: DateTime<Utc>,
}

#[async_trait]
pub(crate) trait RepositorioPoliticas<C: Send>: Send + Sync + fmt::Debug {
    /// Todas las versiones, de la más reciente a la más antigua.
    async fn listar(&self, cx: &mut C) -> Result<Vec<PoliticaGuardada>, ErrorPersistencia>;
    /// Falla con `Duplicado { restriccion: "politica_version_uk" }` si la versión ya existe.
    async fn publicar(&self, cx: &mut C, politica: &NuevaPolitica) -> Result<i64, ErrorPersistencia>;
}

/// Restricción única de la fecha de un día especial (ver la migración del módulo).
pub(crate) const RESTRICCION_FECHA_UNICA: &str = "dia_especial_fecha_uk";

/// Puertos que comparten los casos de uso del módulo.
#[derive(Debug)]
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) negocio: Arc<dyn RepositorioNegocio<C>>,
    pub(crate) dias: Arc<dyn RepositorioDiasEspeciales<C>>,
    pub(crate) horario: Arc<dyn ConsultaHorarioAtencion<C>>,
    pub(crate) politicas: Arc<dyn RepositorioPoliticas<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}
