use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use barberos::api::EquipoAgenda;
use chrono::{DateTime, NaiveDate, Utc};
use clientes::api::ClientesAgenda;
use negocio::api::{CalendarioNegocio, ParametrosNegocio};
use notificaciones::api::Avisos;
use nucleo::identificadores::IdBarbero;
use nucleo::{ErrorPersistencia, Reloj, Transacciones};

use super::dto::{
    BarberoDto, ClienteDestacadoDto, ClienteReactivarDto, ClientesPeriodoDto, MetodoPagoDto,
    ServicioVendidoDto, VentaDetalleDto,
};

/// Sumas de ventas vigentes, tal como salen de `ventas.v_venta`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Sumas {
    pub(crate) ventas: i64,
    pub(crate) servicios: i64,
    pub(crate) subtotal: i64,
    pub(crate) descuentos: i64,
    pub(crate) ventas_netas: i64,
    pub(crate) propinas: i64,
    pub(crate) total_cobrado: i64,
    pub(crate) comisiones: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilaBarbero {
    pub(crate) barbero_id: i64,
    pub(crate) nombre: String,
    pub(crate) sumas: Sumas,
}

/// Cita con los nombres para mostrarla (de `agenda.v_cita` y las vistas de los demás módulos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CitaVista {
    pub(crate) id: i64,
    pub(crate) inicio: DateTime<Utc>,
    pub(crate) fin: DateTime<Utc>,
    pub(crate) estado: String,
    /// `None`: cliente anonimizado.
    pub(crate) cliente: Option<String>,
    pub(crate) servicio: String,
    pub(crate) barbero_id: i64,
    pub(crate) barbero: String,
}

/// Lecturas de las vistas publicadas (`v_`) de los demás módulos (regla 4 del plan): los
/// reportes no tienen esquema propio ni escriben nada.
#[async_trait]
pub(crate) trait LecturasReportes<C: Send>: Send + Sync + fmt::Debug {
    async fn sumas(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<Sumas, ErrorPersistencia>;
    /// Sumas de un día hasta un instante (comparación «a la misma hora», RF-DAS-02).
    async fn sumas_hasta(
        &self,
        cx: &mut C,
        fecha: NaiveDate,
        hasta: DateTime<Utc>,
    ) -> Result<Sumas, ErrorPersistencia>;
    async fn netas_por_dia(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(NaiveDate, i64)>, ErrorPersistencia>;
    async fn por_servicio(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<Vec<ServicioVendidoDto>, ErrorPersistencia>;
    async fn por_metodo(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<MetodoPagoDto>, ErrorPersistencia>;
    async fn por_barbero(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<FilaBarbero>, ErrorPersistencia>;
    async fn barbero(&self, cx: &mut C, id: IdBarbero) -> Result<Option<BarberoDto>, ErrorPersistencia>;
    /// Barberos con ventas en el periodo o vigentes en él (para el reporte de todos).
    async fn barberos_del_periodo(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<IdBarbero>, ErrorPersistencia>;
    async fn ventas_de_barbero(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: IdBarbero,
    ) -> Result<Vec<VentaDetalleDto>, ErrorPersistencia>;
    async fn clientes_distintos(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: IdBarbero,
    ) -> Result<i64, ErrorPersistencia>;
    /// Atendidos, ocasionales, nuevos y con retorno (sin los porcentajes).
    async fn clientes_del_periodo(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<ClientesPeriodoDto, ErrorPersistencia>;
    async fn inasistencias(
        &self,
        cx: &mut C,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
    ) -> Result<i64, ErrorPersistencia>;
    async fn mas_visitas(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
        limite: i64,
    ) -> Result<Vec<ClienteDestacadoDto>, ErrorPersistencia>;
    /// Clientes cuya última visita fue antes de `ultima_antes_de`, los más recientes primero.
    async fn por_reactivar(
        &self,
        cx: &mut C,
        ultima_antes_de: NaiveDate,
        hoy: NaiveDate,
        limite: i64,
    ) -> Result<(Vec<ClienteReactivarDto>, i64), ErrorPersistencia>;
    /// Citas que empiezan en `[desde, hasta)`, con nombres; `estados` vacío: todas.
    async fn citas(
        &self,
        cx: &mut C,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
        estados: &[&str],
    ) -> Result<Vec<CitaVista>, ErrorPersistencia>;
    /// Clientes que cumplen años en alguno de esos días (`(mes, día)`): `(id, nombre, celular,
    /// nacimiento)`.
    async fn cumpleanos(
        &self,
        cx: &mut C,
        dias: &[(u32, u32)],
    ) -> Result<Vec<(i64, String, String, NaiveDate)>, ErrorPersistencia>;
    /// Clientes atendidos (ventas) por mes de un año: `(mes, cantidad)`.
    async fn atendidos_por_mes(&self, cx: &mut C, anio: i32) -> Result<Vec<(u32, i64)>, ErrorPersistencia>;
    /// Ventas por día de la semana (1 = lunes) y hora de Bogotá: `(día, hora, cantidad)`.
    async fn ventas_por_hora(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(u8, u32, i64)>, ErrorPersistencia>;
    async fn clientes_inactivos(
        &self,
        cx: &mut C,
        ultima_antes_de: NaiveDate,
    ) -> Result<i64, ErrorPersistencia>;
}

/// Documento listo para convertir en PDF: HTML del cuerpo, del encabezado y del pie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentoPdf {
    pub(crate) cuerpo: String,
    pub(crate) encabezado: String,
    pub(crate) pie: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub(crate) struct ErrorPdf(pub(crate) String);

/// Conversión HTML → PDF (patrón Adapter): Gotenberg en producción.
#[async_trait]
pub(crate) trait GeneradorPdf: Send + Sync + fmt::Debug {
    async fn generar(&self, documento: &DocumentoPdf) -> Result<Vec<u8>, ErrorPdf>;
}

pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) lecturas: Arc<dyn LecturasReportes<C>>,
    pub(crate) equipo: Arc<dyn EquipoAgenda<C>>,
    pub(crate) calendario: Arc<dyn CalendarioNegocio<C>>,
    pub(crate) negocio: Arc<dyn ParametrosNegocio<C>>,
    pub(crate) clientes: Arc<dyn ClientesAgenda<C>>,
    pub(crate) avisos: Arc<dyn Avisos<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    /// `None`: sin Gotenberg configurado (los PDF responden 503).
    pub(crate) pdf: Option<Arc<dyn GeneradorPdf>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosReportes")
    }
}
