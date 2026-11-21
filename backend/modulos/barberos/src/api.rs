//! Fachada pública del módulo `barberos`. Otros módulos (p. ej. `agenda` en la fase 3) leerán
//! la disponibilidad del equipo a través de aquí.

pub use crate::aplicacion::dto::{
    AusenciaDto, BarberoDto, BloqueoDto, DatosPrivadosDto, DocumentoDto, EstadoDto, ExpedienteDto,
    HorarioDto, ListadoBarberosDto, ServicioOfrecidoDto,
};
pub use crate::aplicacion::errores::ErrorBarberos;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nucleo::identificadores::{IdBarbero, IdServicio};

/// Franja de trabajo (1 = lunes … 7 = domingo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FranjaAgenda {
    pub dia_semana: u8,
    pub inicio: NaiveTime,
    pub fin: NaiveTime,
}

/// Ausencia vigente (no anulada): `tipo` es `vacaciones`, `incapacidad`, `licencia` o
/// `suspension`. Fechas incluidas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AusenciaAgenda {
    pub tipo: String,
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
}

/// Lo que la agenda y el sitio público necesitan de un barbero para un rango de fechas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarberoAgenda {
    pub id: IdBarbero,
    pub nombre: String,
    pub nombres: String,
    pub especialidad: String,
    pub anios_experiencia: Option<i16>,
    pub instagram: Option<String>,
    pub biografia: Option<String>,
    pub foto_url: Option<String>,
    /// Orden del equipo (landing y «Cualquier barbero», RN-06).
    pub orden: i16,
    pub mostrar_en_landing: bool,
    pub recibe_citas_en_linea: bool,
    pub fecha_retiro: Option<NaiveDate>,
    pub servicios: Vec<IdServicio>,
    pub franjas: Vec<FranjaAgenda>,
    /// Ausencias que se cruzan con el rango consultado.
    pub ausencias: Vec<AusenciaAgenda>,
    /// Bloqueos puntuales que se cruzan con el rango consultado.
    pub bloqueos: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

/// Barbero tal como lo necesita una venta (RF-VEN-04, RN-05, RN-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarberoVenta {
    pub id: IdBarbero,
    pub nombre: String,
    /// Sin retiro ni ausencia en la fecha de la venta.
    pub activo: bool,
    pub servicios: Vec<IdServicio>,
    /// Su comisión vigente: la venta guarda una copia (RNF-INT-05).
    pub porcentaje_comision: nucleo::Porcentaje,
}

/// Fachada para `ventas`, en la transacción de quien llama.
#[async_trait]
pub trait EquipoVentas<C: Send>: Send + Sync + std::fmt::Debug {
    async fn para_venta(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        fecha: NaiveDate,
    ) -> Result<Option<BarberoVenta>, nucleo::ErrorPersistencia>;

    /// Nombres completos, también de los retirados (para el historial de ventas).
    async fn nombres(
        &self,
        cx: &mut C,
        ids: &[IdBarbero],
    ) -> Result<std::collections::HashMap<IdBarbero, String>, nucleo::ErrorPersistencia>;
}

/// Fachada para `agenda`: el equipo con su horario, ausencias y bloqueos en un rango de fechas
/// (incluidas), en el orden del equipo. No incluye a los retirados antes de `desde`.
#[async_trait]
pub trait EquipoAgenda<C: Send>: Send + Sync + std::fmt::Debug {
    async fn equipo(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<BarberoAgenda>, ErrorBarberos>;

    /// Nombre completo de cada barbero de la lista, incluidos los retirados (historial de citas).
    async fn nombres(
        &self,
        cx: &mut C,
        ids: &[IdBarbero],
    ) -> Result<std::collections::HashMap<IdBarbero, String>, ErrorBarberos>;
}
