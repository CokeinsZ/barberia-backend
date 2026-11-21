use std::fmt;
use std::sync::Arc;

use agenda::api::CitasParaVentas;
use async_trait::async_trait;
use auditoria::api::Auditor;
use barberos::api::EquipoVentas;
use catalogo::api::CatalogoServicios;
use chrono::{DateTime, NaiveDate, Utc};
use clientes::api::ClientesAgenda;
use nucleo::eventos::BusEventos;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio, IdUsuario, IdVenta};
use nucleo::{Dinero, ErrorPersistencia, Porcentaje, Reloj, SolicitudPagina, Transacciones};

use crate::dominio::TipoDescuento;

pub(crate) const RESTRICCION_CITA_COBRADA: &str = "venta_cita_uk";
pub(crate) const RESTRICCION_CIERRE: &str = "cierre_caja_fecha_uk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetodoPago {
    pub(crate) id: i16,
    pub(crate) codigo: String,
    pub(crate) nombre: String,
    pub(crate) activo: bool,
    pub(crate) orden: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Descuento {
    pub(crate) id: i16,
    pub(crate) nombre: String,
    pub(crate) tipo: TipoDescuento,
    pub(crate) permitido_a_barberos: bool,
    pub(crate) activo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Motivo {
    pub(crate) id: i16,
    pub(crate) nombre: String,
    pub(crate) activo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevaVenta {
    pub(crate) numero_ticket: i64,
    pub(crate) registrada_en: DateTime<Utc>,
    pub(crate) registrada_por: IdUsuario,
    pub(crate) barbero: IdBarbero,
    pub(crate) cliente: Option<IdCliente>,
    pub(crate) cita: Option<IdCita>,
    /// `(descuento, motivo, valor aplicado)`.
    pub(crate) descuento: Option<(i16, i16, Dinero)>,
    pub(crate) propina: Dinero,
    pub(crate) porcentaje_comision: Porcentaje,
    /// `(servicio, precio vigente)` en orden.
    pub(crate) lineas: Vec<(IdServicio, Dinero)>,
    pub(crate) pagos: Vec<(i16, Dinero)>,
}

/// Venta leída de la base, con sus líneas y pagos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VentaGuardada {
    pub(crate) id: IdVenta,
    pub(crate) numero_ticket: i64,
    pub(crate) registrada_en: DateTime<Utc>,
    pub(crate) fecha_operacion: NaiveDate,
    pub(crate) registrada_por: IdUsuario,
    pub(crate) barbero: IdBarbero,
    pub(crate) cliente: Option<IdCliente>,
    pub(crate) cita: Option<IdCita>,
    pub(crate) descuento: Option<(i16, i16, Dinero)>,
    pub(crate) propina: Dinero,
    pub(crate) porcentaje_comision: Porcentaje,
    pub(crate) estado: String,
    /// `(anulada_en, anulada_por, motivo)`.
    pub(crate) anulacion: Option<(DateTime<Utc>, IdUsuario, String)>,
    pub(crate) lineas: Vec<(i16, IdServicio, Dinero)>,
    pub(crate) pagos: Vec<(i16, Dinero)>,
}

impl VentaGuardada {
    pub(crate) fn registrada(&self) -> bool {
        self.estado == "registrada"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FiltroVentas {
    pub(crate) desde: Option<NaiveDate>,
    pub(crate) hasta: Option<NaiveDate>,
    pub(crate) barbero: Option<IdBarbero>,
    pub(crate) cliente: Option<IdCliente>,
    /// `false`: también las anuladas.
    pub(crate) solo_registradas: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevoCierre {
    pub(crate) fecha: NaiveDate,
    pub(crate) cerrado_por: IdUsuario,
    pub(crate) cerrado_en: DateTime<Utc>,
    pub(crate) observacion: Option<String>,
    /// `(método, esperado, contado)`.
    pub(crate) detalle: Vec<(i16, Dinero, Option<Dinero>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CierreGuardado {
    pub(crate) id: i64,
    pub(crate) fecha: NaiveDate,
    pub(crate) cerrado_por: IdUsuario,
    pub(crate) cerrado_en: DateTime<Utc>,
    pub(crate) observacion: Option<String>,
    pub(crate) detalle: Vec<(i16, Dinero, Option<Dinero>)>,
}

#[async_trait]
pub(crate) trait RepositorioVentas<C: Send>: Send + Sync + fmt::Debug {
    async fn metodos_pago(&self, cx: &mut C) -> Result<Vec<MetodoPago>, ErrorPersistencia>;
    async fn cambiar_metodo(
        &self,
        cx: &mut C,
        id: i16,
        activo: bool,
    ) -> Result<Option<MetodoPago>, ErrorPersistencia>;
    async fn descuentos(&self, cx: &mut C) -> Result<Vec<Descuento>, ErrorPersistencia>;
    async fn motivos(&self, cx: &mut C) -> Result<Vec<Motivo>, ErrorPersistencia>;
    /// Siguiente número de ticket, sin huecos: bloquea el contador hasta el final de la
    /// transacción (RN-19, RNF-INT-07).
    async fn siguiente_ticket(&self, cx: &mut C) -> Result<i64, ErrorPersistencia>;
    async fn insertar(&self, cx: &mut C, venta: &NuevaVenta) -> Result<IdVenta, ErrorPersistencia>;
    async fn obtener(
        &self,
        cx: &mut C,
        id: IdVenta,
        bloquear: bool,
    ) -> Result<Option<VentaGuardada>, ErrorPersistencia>;
    async fn listar(
        &self,
        cx: &mut C,
        filtro: &FiltroVentas,
        pagina: SolicitudPagina,
    ) -> Result<(Vec<VentaGuardada>, i64), ErrorPersistencia>;
    async fn anular(
        &self,
        cx: &mut C,
        id: IdVenta,
        por: IdUsuario,
        motivo: &str,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
    /// Última visita del cliente entre sus ventas vigentes.
    async fn ultima_visita_vigente(
        &self,
        cx: &mut C,
        cliente: IdCliente,
    ) -> Result<Option<NaiveDate>, ErrorPersistencia>;
    /// Lo cobrado por cada método en las ventas vigentes del día, propinas incluidas.
    async fn cobrado_por_metodo(
        &self,
        cx: &mut C,
        fecha: NaiveDate,
    ) -> Result<Vec<(i16, Dinero)>, ErrorPersistencia>;
    async fn cierre(&self, cx: &mut C, fecha: NaiveDate)
    -> Result<Option<CierreGuardado>, ErrorPersistencia>;
    async fn crear_cierre(&self, cx: &mut C, cierre: &NuevoCierre) -> Result<i64, ErrorPersistencia>;
}

pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) ventas: Arc<dyn RepositorioVentas<C>>,
    pub(crate) citas: Arc<dyn CitasParaVentas<C>>,
    pub(crate) equipo: Arc<dyn EquipoVentas<C>>,
    pub(crate) catalogo: Arc<dyn CatalogoServicios<C>>,
    pub(crate) clientes: Arc<dyn ClientesAgenda<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) bus: Arc<BusEventos<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosVentas")
    }
}
