//! Puertos: lo que los casos de uso necesitan del exterior, como traits pequeños.
//! Las implementaciones viven en `infraestructura` (PostgreSQL) y en las pruebas (memoria).

use async_trait::async_trait;
use chrono::NaiveDate;
use nucleo::ErrorPersistencia;
use nucleo::identificadores::IdDiaEspecial;

use crate::dominio::entidades::{DatosNegocio, DiaEspecial};

#[async_trait]
pub(crate) trait RepositorioNegocio: Send + Sync + std::fmt::Debug {
    async fn obtener(&self) -> Result<DatosNegocio, ErrorPersistencia>;
    async fn guardar(&self, datos: &DatosNegocio) -> Result<(), ErrorPersistencia>;
}

/// Día especial ya guardado, con su identificador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiaEspecialGuardado {
    pub(crate) id: IdDiaEspecial,
    pub(crate) dia: DiaEspecial,
}

#[async_trait]
pub(crate) trait RepositorioDiasEspeciales: Send + Sync + std::fmt::Debug {
    /// Días entre dos fechas (incluidas), ordenados por fecha.
    async fn listar(
        &self,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialGuardado>, ErrorPersistencia>;
    /// Falla con `Duplicado { restriccion: "dia_especial_fecha_uk" }` si la fecha ya existe.
    async fn registrar(&self, dia: &DiaEspecial) -> Result<IdDiaEspecial, ErrorPersistencia>;
    /// Devuelve `false` si no existía.
    async fn eliminar(&self, id: IdDiaEspecial) -> Result<bool, ErrorPersistencia>;
}

/// Restricción única de la fecha de un día especial (ver la migración del módulo).
pub(crate) const RESTRICCION_FECHA_UNICA: &str = "dia_especial_fecha_uk";
