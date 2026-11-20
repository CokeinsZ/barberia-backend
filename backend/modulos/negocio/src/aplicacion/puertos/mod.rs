//! Puertos: lo que los casos de uso necesitan del exterior, como traits pequeños y genéricos
//! en el contexto transaccional `C` (ADR 0005). Las implementaciones viven en
//! `infraestructura` (PostgreSQL, `C = UnidadDeTrabajo`) y en las pruebas (memoria, `C = ()`).

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use chrono::NaiveDate;
use nucleo::identificadores::IdDiaEspecial;
use nucleo::{ErrorPersistencia, Reloj, Transacciones};

use crate::dominio::entidades::{DatosNegocio, DiaEspecial};

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
}

/// Restricción única de la fecha de un día especial (ver la migración del módulo).
pub(crate) const RESTRICCION_FECHA_UNICA: &str = "dia_especial_fecha_uk";

/// Puertos que comparten los casos de uso del módulo.
#[derive(Debug)]
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) negocio: Arc<dyn RepositorioNegocio<C>>,
    pub(crate) dias: Arc<dyn RepositorioDiasEspeciales<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}
