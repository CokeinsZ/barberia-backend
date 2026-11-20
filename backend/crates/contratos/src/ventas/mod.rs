//! Eventos que publica el módulo `ventas`.

use chrono::NaiveDate;
use nucleo::Dinero;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdVenta};

use crate::evento;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VentaRegistrada {
    pub venta: IdVenta,
    pub barbero: IdBarbero,
    pub cliente: Option<IdCliente>,
    pub cita: Option<IdCita>,
    pub venta_neta: Dinero,
    pub fecha_operacion: NaiveDate,
}
evento!(VentaRegistrada, "ventas.venta_registrada");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VentaAnulada {
    pub venta: IdVenta,
    pub barbero: IdBarbero,
    pub cliente: Option<IdCliente>,
}
evento!(VentaAnulada, "ventas.venta_anulada");
