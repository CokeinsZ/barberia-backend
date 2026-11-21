//! Datos que salen de los casos de uso (los ve la API HTTP).

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nucleo::identificadores::{IdAusencia, IdBarbero, IdBloqueo, IdDocumento, IdServicio, IdUsuario};
use nucleo::{Porcentaje, ZONA_HORARIA};
use serde::{Deserialize, Serialize};

use crate::dominio::{EstadoBarbero, Horario};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicioOfrecidoDto {
    pub id: IdServicio,
    pub nombre: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EstadoDto {
    /// `activo`, `vacaciones`, `incapacidad_licencia`, `suspendido` o `inactivo`.
    pub codigo: &'static str,
    pub etiqueta: &'static str,
    /// Rango de la ausencia vigente, si la hay (p. ej. «De vacaciones hasta…»).
    pub desde: Option<NaiveDate>,
    pub hasta: Option<NaiveDate>,
}

impl EstadoDto {
    pub(crate) fn nuevo(estado: EstadoBarbero, desde: Option<NaiveDate>, hasta: Option<NaiveDate>) -> Self {
        let etiqueta = match estado {
            EstadoBarbero::Activo => "Activo",
            EstadoBarbero::Vacaciones => "Vacaciones",
            EstadoBarbero::IncapacidadLicencia => "Incapacidad o licencia",
            EstadoBarbero::Suspendido => "Suspendido",
            EstadoBarbero::Inactivo => "Inactivo",
        };
        Self {
            codigo: estado.codigo(),
            etiqueta,
            desde,
            hasta,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisibilidadDto {
    pub mostrar_en_landing: bool,
    pub recibe_citas_en_linea: bool,
    pub acceso_panel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CuentaDto {
    pub correo: String,
    /// Invitado que todavía no ha creado su contraseña.
    pub pendiente_activacion: bool,
    pub ultimo_acceso_en: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FranjaDto {
    pub dia_semana: u8,
    pub inicio: NaiveTime,
    pub fin: NaiveTime,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HorarioDto {
    pub franjas: Vec<FranjaDto>,
    pub total_horas_semana: f64,
}

impl From<&Horario> for HorarioDto {
    fn from(h: &Horario) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let total_horas_semana = h.minutos_semana() as f64 / 60.0;
        Self {
            franjas: h
                .franjas()
                .iter()
                .map(|f| FranjaDto {
                    dia_semana: f.dia_semana,
                    inicio: f.inicio,
                    fin: f.fin,
                })
                .collect(),
            total_horas_semana,
        }
    }
}

/// Barbero en el listado y en la ficha (la ficha trae además el horario).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarberoDto {
    pub id: IdBarbero,
    pub usuario_id: IdUsuario,
    pub nombre: String,
    pub nombres: String,
    pub apellidos: String,
    pub tipo_documento: String,
    pub numero_documento: String,
    pub fecha_nacimiento: NaiveDate,
    pub celular: String,
    pub especialidad: String,
    pub anios_experiencia: Option<i16>,
    pub instagram: Option<String>,
    pub biografia: Option<String>,
    pub foto_url: Option<String>,
    pub fecha_ingreso: NaiveDate,
    pub tipo_vinculacion: String,
    pub fin_contrato: Option<NaiveDate>,
    pub porcentaje_comision: Porcentaje,
    pub periodo_liquidacion: String,
    pub estado: EstadoDto,
    pub fecha_retiro: Option<NaiveDate>,
    pub motivo_retiro: Option<String>,
    pub visibilidad: VisibilidadDto,
    pub cuenta: Option<CuentaDto>,
    pub orden: i16,
    pub servicios: Vec<ServicioOfrecidoDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horario: Option<HorarioDto>,
}

/// Indicadores del equipo (RF-BAR-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndicadoresEquipoDto {
    pub registrados: usize,
    pub activos: usize,
    pub ausencias_temporales: usize,
    pub inactivos: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListadoBarberosDto {
    pub datos: Vec<BarberoDto>,
    pub meta: IndicadoresEquipoDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AusenciaDto {
    pub id: IdAusencia,
    pub tipo: String,
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
    pub observacion: Option<String>,
    /// Descarga firmada del soporte (5 minutos).
    pub soporte_url: Option<String>,
    pub registrada_en: DateTime<Utc>,
    pub anulada: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BloqueoDto {
    pub id: IdBloqueo,
    /// Fecha y horas en Bogotá.
    pub fecha: NaiveDate,
    pub desde: NaiveTime,
    pub hasta: NaiveTime,
    pub inicio: DateTime<Utc>,
    pub fin: DateTime<Utc>,
    pub motivo: String,
    pub nota: Option<String>,
}

impl BloqueoDto {
    pub(crate) fn nuevo(
        id: IdBloqueo,
        inicio: DateTime<Utc>,
        fin: DateTime<Utc>,
        motivo: String,
        nota: Option<String>,
    ) -> Self {
        let (i, f) = (
            inicio.with_timezone(&ZONA_HORARIA),
            fin.with_timezone(&ZONA_HORARIA),
        );
        Self {
            id,
            fecha: i.date_naive(),
            desde: i.time(),
            hasta: f.time(),
            inicio,
            fin,
            motivo,
            nota,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentoDto {
    pub id: IdDocumento,
    pub tipo: String,
    pub periodo: Option<NaiveDate>,
    pub nombre_original: String,
    pub tamano_bytes: usize,
    pub cargado_en: DateTime<Utc>,
    /// Descarga firmada (5 minutos).
    pub url_descarga: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpedienteDto {
    pub documentos: Vec<DocumentoDto>,
    /// Documentos requeridos que faltan (RF-BAR-06).
    pub faltantes: Vec<&'static str>,
    pub completo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContactoEmergenciaDto {
    pub nombre: Option<String>,
    pub parentesco: Option<String>,
    pub celular: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatosPrivadosDto {
    pub direccion: Option<String>,
    pub ciudad: Option<String>,
    pub grupo_sanguineo: Option<String>,
    pub emergencia: ContactoEmergenciaDto,
    pub eps: Option<String>,
    pub fondo_pensiones: Option<String>,
    pub arl: Option<String>,
    pub caja_compensacion: Option<String>,
    pub medio_pago: Option<String>,
    pub banco: Option<String>,
    pub tipo_cuenta: Option<String>,
    pub numero_cuenta: Option<String>,
}

/// Foto nueva.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FotoDto {
    pub foto_url: String,
}
