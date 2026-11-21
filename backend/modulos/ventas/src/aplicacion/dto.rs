//! Datos que salen de los casos de uso. Dinero en pesos enteros; instantes en hora de Bogotá.

use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleo::{Dinero, Porcentaje};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetodoPagoDto {
    pub id: i16,
    pub codigo: String,
    pub nombre: String,
    pub activo: bool,
    pub orden: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescuentoDto {
    pub id: i16,
    pub nombre: String,
    /// `porcentaje` o `valor_fijo`.
    pub tipo: &'static str,
    /// Porcentaje (p. ej. `10`) o pesos (p. ej. `5000`), según el tipo.
    pub valor: i64,
    pub permitido_a_barberos: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotivoDto {
    pub id: i16,
    pub nombre: String,
}

/// Lo que necesita el formulario de venta (RF-VEN-05, RF-VEN-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogosDto {
    pub metodos_pago: Vec<MetodoPagoDto>,
    pub descuentos: Vec<DescuentoDto>,
    pub motivos_descuento: Vec<MotivoDto>,
    /// Valores rápidos de propina (RF-VEN-06).
    pub propinas_rapidas: [i64; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NombreDto {
    pub id: i64,
    pub nombre: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineaDto {
    pub posicion: i16,
    pub servicio_id: i64,
    pub nombre: String,
    pub precio: Dinero,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescuentoAplicadoDto {
    pub descuento_id: i16,
    pub nombre: String,
    pub motivo_descuento_id: i16,
    pub motivo: String,
    pub valor: Dinero,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PagoDto {
    pub metodo_pago_id: i16,
    pub nombre: String,
    pub monto: Dinero,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnulacionDto {
    pub anulada_en: DateTime<FixedOffset>,
    pub anulada_por: i64,
    pub motivo: String,
}

/// Venta con sus líneas, pagos y totales (RF-VEN-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VentaDto {
    pub id: i64,
    pub numero_ticket: i64,
    pub registrada_en: DateTime<FixedOffset>,
    pub fecha_operacion: NaiveDate,
    pub registrada_por: i64,
    /// `registrada` o `anulada`.
    pub estado: String,
    pub barbero: NombreDto,
    /// `None`: cliente ocasional.
    pub cliente: Option<NombreDto>,
    pub cita_id: Option<i64>,
    pub lineas: Vec<LineaDto>,
    pub subtotal: Dinero,
    pub descuento: Option<DescuentoAplicadoDto>,
    pub venta_neta: Dinero,
    pub propina: Dinero,
    pub total_cobrado: Dinero,
    pub porcentaje_comision: Porcentaje,
    pub comision_barbero: Dinero,
    pub pagos: Vec<PagoDto>,
    pub anulacion: Option<AnulacionDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TotalMetodoDto {
    pub metodo_pago_id: i16,
    pub codigo: String,
    pub nombre: String,
    pub total: Dinero,
}

/// Caja del día (RF-VEN-10): las ventas anuladas no suman.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CajaDto {
    pub fecha: NaiveDate,
    pub numero_ventas: usize,
    pub subtotal: Dinero,
    pub descuentos: Dinero,
    /// Ingreso de la barbería (sin propinas).
    pub venta_neta: Dinero,
    pub propinas: Dinero,
    pub total_cobrado: Dinero,
    /// Lo cobrado por cada método, propinas incluidas.
    pub por_metodo: Vec<TotalMetodoDto>,
    pub anuladas: usize,
    /// Las del día, de la más reciente a la más antigua (filtrables por barbero).
    pub ventas: Vec<VentaDto>,
}

/// «Lo que llevas hoy» del barbero (RF-PBA-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumenBarberoDto {
    pub fecha: NaiveDate,
    pub barbero: NombreDto,
    pub numero_ventas: usize,
    pub venta_neta: Dinero,
    pub comision: Dinero,
    pub propinas: Dinero,
    /// Comisión + propinas (RN-15).
    pub total_a_recibir: Dinero,
    pub ventas: Vec<VentaDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CierreMetodoDto {
    pub metodo_pago_id: i16,
    pub nombre: String,
    /// Lo cobrado por ese método según las ventas, propinas incluidas (foto del cierre).
    pub esperado: Dinero,
    /// `None`: no se contó.
    pub contado: Option<Dinero>,
    /// Contado − esperado (negativo si falta dinero).
    pub diferencia: Option<i64>,
}

/// Cierre de caja (RF-VEN-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CierreDto {
    pub id: i64,
    pub fecha: NaiveDate,
    pub cerrado_por: i64,
    pub cerrado_en: DateTime<FixedOffset>,
    pub observacion: Option<String>,
    pub metodos: Vec<CierreMetodoDto>,
    pub total_esperado: Dinero,
    pub total_contado: Dinero,
    pub diferencia: i64,
}
