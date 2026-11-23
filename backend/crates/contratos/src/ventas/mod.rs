//! Eventos que publica el módulo `ventas`.

use chrono::NaiveDate;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdUsuario, IdVenta};
use nucleo::{Dinero, TipoActor};

use crate::evento;

/// Venta registrada: la cita (si la hay) queda «Atendida» y el cliente suma la visita (RN-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VentaRegistrada {
    pub venta: IdVenta,
    /// Quién la registró (barbero o administrador), para el historial de la cita.
    pub registrada_por: IdUsuario,
    pub actor: TipoActor,
    pub barbero: IdBarbero,
    pub cliente: Option<IdCliente>,
    pub cita: Option<IdCita>,
    pub venta_neta: Dinero,
    pub fecha_operacion: NaiveDate,
}
evento!(VentaRegistrada, "ventas.venta_registrada");

/// Una venta anulada sale de los totales. Trae lo necesario para que `clientes` corrija sus
/// métricas sin leer el esquema de ventas, y para que `agenda` devuelva su cita a «por cobrar».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VentaAnulada {
    pub venta: IdVenta,
    /// La administradora que la anuló (solo ella anula, RN-17), para el historial de la cita.
    pub anulada_por: IdUsuario,
    pub barbero: IdBarbero,
    pub cliente: Option<IdCliente>,
    pub cita: Option<IdCita>,
    pub venta_neta: Dinero,
    pub fecha_operacion: NaiveDate,
    /// Última visita del cliente entre sus ventas que siguen vigentes (`None` si no queda ninguna).
    pub ultima_visita_vigente: Option<NaiveDate>,
}

evento!(VentaAnulada, "ventas.venta_anulada");
