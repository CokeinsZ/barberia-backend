//! Datos que salen de los reportes. Dinero en pesos enteros; instantes en hora de Bogotá.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::Serialize;

use crate::dominio::periodo::Periodo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeriodoDto {
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
    /// «1 al 21 de noviembre de 2026».
    pub texto: String,
    pub dias: i64,
}

impl From<Periodo> for PeriodoDto {
    fn from(p: Periodo) -> Self {
        PeriodoDto {
            desde: p.desde,
            hasta: p.hasta,
            texto: p.texto(),
            dias: p.dias(),
        }
    }
}

/// Totales de ventas vigentes (las anuladas no cuentan).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TotalesDto {
    /// Ventas registradas: cada una es un cliente atendido (RN-21).
    pub clientes_atendidos: i64,
    pub servicios: i64,
    pub subtotal: i64,
    pub descuentos: i64,
    /// Subtotal − descuentos: ingreso de la barbería.
    pub ventas_netas: i64,
    pub propinas: i64,
    pub total_cobrado: i64,
    pub comisiones: i64,
    /// Ventas netas ÷ clientes atendidos.
    pub ticket_promedio: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanaDto {
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
    pub ventas_netas: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServicioVendidoDto {
    pub servicio_id: i64,
    pub nombre: String,
    pub categoria: String,
    pub unidades: i64,
    /// Al precio de lista de cada venta, antes de descuentos.
    pub ingresos: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetodoPagoDto {
    pub metodo: String,
    pub nombre: String,
    /// Lo cobrado por ese método, propinas incluidas.
    pub total: i64,
}

/// Fila «por barbero» del resumen y base de la liquidación (RF-REP-02, RN-15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiquidacionDto {
    pub barbero_id: i64,
    pub nombre: String,
    pub clientes_atendidos: i64,
    pub servicios: i64,
    pub ventas_netas: i64,
    /// Suma de la comisión de cada venta con el porcentaje que tenía al venderse.
    pub comision: i64,
    /// 100 % del barbero, sin importar el método de pago (DA-09).
    pub propinas: i64,
    /// Comisión + propinas.
    pub total_a_pagar: i64,
}

/// Resumen de ventas y servicios (RF-REP-02).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResumenDto {
    pub periodo: PeriodoDto,
    pub totales: TotalesDto,
    pub por_semana: Vec<SemanaDto>,
    pub por_servicio: Vec<ServicioVendidoDto>,
    pub por_metodo: Vec<MetodoPagoDto>,
    pub por_barbero: Vec<LiquidacionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarberoDto {
    pub id: i64,
    pub nombre: String,
    pub tipo_documento: String,
    pub numero_documento: String,
    pub tipo_vinculacion: String,
    pub fecha_ingreso: NaiveDate,
    /// Porcentaje vigente hoy (cada venta guarda el suyo).
    pub porcentaje_comision: f64,
    pub fecha_retiro: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VentaDetalleDto {
    pub venta_id: i64,
    pub numero_ticket: i64,
    pub registrada_en: DateTime<FixedOffset>,
    pub cliente: String,
    pub servicios: String,
    pub ventas_netas: i64,
    pub propina: i64,
    pub comision: i64,
}

/// Reporte detallado y liquidación de un barbero (RF-REP-03, RF-PBA-06).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReporteBarberoDto {
    pub periodo: PeriodoDto,
    pub barbero: BarberoDto,
    pub totales: TotalesDto,
    /// Clientes registrados distintos (los ocasionales no se cuentan aquí).
    pub clientes_distintos: i64,
    pub servicios: Vec<ServicioVendidoDto>,
    pub liquidacion: LiquidacionDto,
    pub ventas: Vec<VentaDetalleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteDestacadoDto {
    pub cliente_id: i64,
    pub nombre: String,
    pub celular: Option<String>,
    pub visitas: i64,
    pub ventas_netas: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteReactivarDto {
    pub cliente_id: i64,
    pub nombre: String,
    pub celular: String,
    pub ultima_visita: NaiveDate,
    pub dias_sin_volver: i64,
    pub ultimo_servicio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PorReactivarDto {
    pub dias_inactividad: i64,
    pub total: i64,
    pub clientes: Vec<ClienteReactivarDto>,
}

/// Clientes del periodo (RF-REP-05, RF-DAS-05).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ClientesPeriodoDto {
    /// Clientes registrados distintos con al menos una venta en el periodo.
    pub atendidos: i64,
    /// Ventas a «cliente ocasional».
    pub ocasionales: i64,
    /// Su primera venta fue en el periodo.
    pub nuevos: i64,
    pub recurrentes: i64,
    /// Atendidos que volvieron en menos de 60 días desde su visita anterior (RN-21); dos
    /// tiquetes del mismo día no son un retorno.
    pub con_retorno: i64,
    pub porcentaje_retorno: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReporteClientesDto {
    pub periodo: PeriodoDto,
    pub clientes: ClientesPeriodoDto,
    /// Citas marcadas «No asistió» en el periodo.
    pub inasistencias: i64,
    pub mas_visitas: Vec<ClienteDestacadoDto>,
    pub por_reactivar: PorReactivarDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitaResumenDto {
    pub cita_id: i64,
    pub inicio: DateTime<FixedOffset>,
    pub hora: String,
    pub cliente: String,
    pub servicio: String,
    pub barbero_id: i64,
    pub barbero: String,
    pub estado: String,
}

/// Citas de hoy por estado (RF-DAS-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CitasPorEstadoDto {
    pub atendidas: i64,
    pub en_curso: i64,
    /// Por empezar (sin confirmar o confirmadas).
    pub proximas: i64,
    /// Su hora ya pasó y siguen sin cerrar (ni venta ni inasistencia).
    pub sin_cerrar: i64,
    pub no_asistio: i64,
    pub canceladas: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiaVentasDto {
    pub fecha: NaiveDate,
    pub ventas_netas: i64,
}

/// Indicadores de hoy frente al mismo día de la semana anterior a la misma hora (RF-DAS-01, 02).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparacionDto {
    pub fecha: NaiveDate,
    pub totales: TotalesDto,
    pub variacion_ventas_netas: Option<f64>,
    pub variacion_clientes: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RendimientoBarberoDto {
    pub barbero_id: i64,
    pub nombre: String,
    /// `activo` o el tipo de ausencia (`vacaciones`, `incapacidad`…).
    pub estado: String,
    pub clientes_atendidos: i64,
    pub servicios: i64,
    pub ventas_netas: i64,
    pub propinas: i64,
    pub minutos_ocupados: u32,
    pub minutos_disponibles: u32,
    /// Minutos con citas ÷ minutos disponibles, descontando bloqueos (RN-21).
    pub ocupacion: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AusenteDto {
    pub barbero_id: i64,
    pub nombre: String,
    pub tipo: String,
    pub hasta: NaiveDate,
}

/// Dashboard del día (RF-DAS-01 a 04).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DashboardDto {
    pub fecha: NaiveDate,
    pub generado_en: DateTime<FixedOffset>,
    pub hoy: TotalesDto,
    pub comparacion: ComparacionDto,
    /// Ventas netas de los últimos 12 días, hoy incluido.
    pub tendencia: Vec<DiaVentasDto>,
    pub citas: CitasPorEstadoDto,
    pub proximas: Vec<CitaResumenDto>,
    pub barberos: Vec<RendimientoBarberoDto>,
    pub totales_barberos: TotalesDto,
    pub ausentes: Vec<AusenteDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MesClientesDto {
    pub mes: u32,
    pub anio_actual: i64,
    pub anio_anterior: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HoraPicoDto {
    /// 1 = lunes … 7 = domingo.
    pub dia_semana: u8,
    pub hora: u32,
    /// Clientes atendidos por semana, en promedio de las últimas 4.
    pub promedio: f64,
}

/// Tendencias del mes (RF-DAS-05, 06 y 07).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TendenciasDto {
    pub periodo: PeriodoDto,
    pub por_servicio: Vec<ServicioVendidoDto>,
    pub clientes: ClientesPeriodoDto,
    pub clientes_por_mes: Vec<MesClientesDto>,
    pub horas_pico: Vec<HoraPicoDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CumpleanosDto {
    pub cliente_id: i64,
    pub nombre: String,
    pub celular: String,
    pub fecha: NaiveDate,
}

/// Alertas y centro de notificaciones (RF-DAS-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlertasDto {
    pub sin_confirmar_manana: Vec<CitaResumenDto>,
    pub cumpleanos: Vec<CumpleanosDto>,
    pub ausentes: Vec<AusenteDto>,
    pub por_reactivar: i64,
    /// Solicitudes del titular (consultas, correcciones, supresiones) sin responder.
    pub solicitudes_titular: i64,
    /// Avisos internos sin leer: cambios de horario, bloqueos o ausencias con citas afectadas
    /// y mensajes fallidos (se listan en `/avisos`).
    pub avisos_no_leidos: i64,
}
