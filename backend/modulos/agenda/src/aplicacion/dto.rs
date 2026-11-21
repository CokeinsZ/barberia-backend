//! Datos que salen de los casos de uso. Las fechas y horas van en la zona de Bogotá.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use nucleo::Dinero;
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HoraLibreDto {
    pub inicio: DateTime<FixedOffset>,
    /// `15:30`.
    pub hora: String,
    /// Con «Cualquier barbero», el barbero que se asignaría (RF-SIS-02).
    pub barbero_id: IdBarbero,
    pub barbero: String,
}

/// Horas libres de un día, agrupadas (RF-RES-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HorasDelDiaDto {
    pub fecha: NaiveDate,
    pub manana: Vec<HoraLibreDto>,
    pub tarde: Vec<HoraLibreDto>,
    pub noche: Vec<HoraLibreDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiaCalendarioDto {
    pub fecha: NaiveDate,
    /// `disponible`, `ultimos_cupos`, `sin_cupos` o `fuera_de_ventana`.
    pub estado: &'static str,
    pub horas_libres: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarioDto {
    pub mes: String,
    pub dias: Vec<DiaCalendarioDto>,
}

/// Opción de barbero en el paso 2 (RF-RES-03, RF-RES-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpcionBarberoDto {
    pub id: IdBarbero,
    pub nombre: String,
    pub especialidad: String,
    pub foto_url: Option<String>,
    pub disponible: bool,
    /// Por qué no se puede elegir (p. ej. «No realiza este servicio»).
    pub motivo: Option<String>,
    pub proxima_hora: Option<DateTime<FixedOffset>>,
}

/// Barbero del sitio público (RF-PUB-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BarberoPublicoDto {
    pub id: IdBarbero,
    pub nombre: String,
    pub nombre_corto: String,
    pub especialidad: String,
    pub anios_experiencia: Option<i16>,
    pub instagram: Option<String>,
    pub biografia: Option<String>,
    pub foto_url: Option<String>,
    /// «De vacaciones hasta…»: se muestra sin botón de agendar.
    pub de_vacaciones_hasta: Option<NaiveDate>,
    pub agendable: bool,
}

/// Todo lo que la landing necesita en una llamada (RF-PUB-01 a 04, RF-CFG-03).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandingDto {
    pub negocio: negocio::api::NegocioPublicoDto,
    pub horario_atencion: Vec<negocio::api::HorarioDiaDto>,
    pub servicios: Vec<catalogo::api::ServicioDto>,
    pub barberos: Vec<BarberoPublicoDto>,
    pub politicas: Vec<negocio::api::PoliticaDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServicioCitaDto {
    pub id: IdServicio,
    pub nombre: String,
    pub duracion_min: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BarberoCitaDto {
    pub id: IdBarbero,
    pub nombre: String,
    pub asignacion_automatica: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteCitaDto {
    pub id: IdCliente,
    pub nombre: String,
    pub celular: String,
    pub visitas: i32,
    pub inasistencias: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitaDto {
    pub id: IdCita,
    pub estado: &'static str,
    /// `en_linea` o `panel`.
    pub origen: String,
    pub inicio: DateTime<FixedOffset>,
    pub fin: DateTime<FixedOffset>,
    /// Precio vigente al agendar (RF-SER-03): el total a pagar en el local.
    pub precio: Dinero,
    pub servicio: ServicioCitaDto,
    pub barbero: BarberoCitaDto,
    /// Solo en las vistas del personal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cliente: Option<ClienteCitaDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nota: Option<String>,
    pub cancelacion_tardia: Option<bool>,
    pub motivo_cancelacion: Option<String>,
}

/// Respuesta de la reserva en línea (sección 3.1 del diseño de la API).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitaCreadaDto {
    pub cita: CitaDto,
    pub enlace_gestion: String,
    pub calendario_google: String,
}

/// Página de gestión por enlace (RF-RES-14, 15 y 18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GestionCitaDto {
    pub cita: CitaDto,
    pub direccion: Option<String>,
    pub puede_cancelar: bool,
    pub puede_reprogramar: bool,
    /// Si se cancela ahora, quedará marcada como tardía (RN-09).
    pub cancelacion_seria_tardia: bool,
    pub recordatorios_activos: bool,
    pub calendario_google: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventoCitaDto {
    pub evento: String,
    pub actor: String,
    pub ocurrido_en: DateTime<FixedOffset>,
    pub inicio_previo: Option<DateTime<FixedOffset>>,
    pub barbero_previo: Option<IdBarbero>,
    pub tardio: bool,
    pub detalle: Option<String>,
}

/// Detalle para el personal (RF-AGE-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetalleCitaDto {
    #[serde(flatten)]
    pub cita: CitaDto,
    pub historial: Vec<EventoCitaDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RangoHorasDto {
    pub inicio: NaiveTime,
    pub fin: NaiveTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RangoInstantesDto {
    pub inicio: DateTime<FixedOffset>,
    pub fin: DateTime<FixedOffset>,
}

/// Columna del tablero: un barbero (RF-AGE-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColumnaTableroDto {
    pub barbero_id: IdBarbero,
    pub nombre: String,
    /// `trabaja`, `no_trabaja` (día libre o cerrado) o el tipo de ausencia.
    pub estado: String,
    /// Franjas del día (fuera de ellas, «fuera de turno»).
    pub franjas: Vec<RangoHorasDto>,
    pub bloqueos: Vec<RangoInstantesDto>,
    pub citas: Vec<CitaDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableroDto {
    pub fecha: NaiveDate,
    /// `festivo`, `cerrado`, `cierre_temprano` o `normal`.
    pub tipo_dia: &'static str,
    pub total_citas: usize,
    pub por_atender: usize,
    pub barberos: Vec<ColumnaTableroDto>,
}
