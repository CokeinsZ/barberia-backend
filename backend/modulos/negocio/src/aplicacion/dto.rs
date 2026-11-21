//! Datos que salen de los casos de uso. Son los mismos que ven la API HTTP y los demás módulos.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nucleo::Porcentaje;
use nucleo::identificadores::IdDiaEspecial;
use serde::Serialize;

use crate::dominio::entidades::{DatosNegocio, DiaEspecial, OrigenDiaEspecial, TipoDiaEspecial};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatosNegocioDto {
    pub nombre_comercial: String,
    pub nit: Option<String>,
    pub direccion: Option<String>,
    pub zona_referencia: Option<String>,
    /// Celular de 10 dígitos, sin indicativo.
    pub whatsapp: Option<String>,
    pub instagram: Option<String>,
    pub porcentaje_comision_defecto: Porcentaje,
}

impl From<&DatosNegocio> for DatosNegocioDto {
    fn from(d: &DatosNegocio) -> Self {
        Self {
            nombre_comercial: d.nombre_comercial().to_owned(),
            nit: d.nit().map(str::to_owned),
            direccion: d.direccion().map(str::to_owned),
            zona_referencia: d.zona_referencia().map(str::to_owned),
            whatsapp: d.whatsapp().map(|c| c.digitos().to_owned()),
            instagram: d.instagram().map(str::to_owned),
            porcentaje_comision_defecto: d.porcentaje_comision_defecto(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDiaEspecialDto {
    Festivo,
    Cerrado,
    CierreTemprano,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiaEspecialDto {
    pub id: IdDiaEspecial,
    pub fecha: NaiveDate,
    pub nombre: String,
    pub tipo: TipoDiaEspecialDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hora_cierre: Option<NaiveTime>,
    /// `true` si lo precargó el sistema (festivos de Colombia).
    pub automatico: bool,
}

impl DiaEspecialDto {
    pub(crate) fn desde(id: IdDiaEspecial, dia: &DiaEspecial) -> Self {
        Self {
            id,
            fecha: dia.fecha(),
            nombre: dia.nombre().to_owned(),
            tipo: match dia.tipo() {
                TipoDiaEspecial::Festivo => TipoDiaEspecialDto::Festivo,
                TipoDiaEspecial::Cerrado => TipoDiaEspecialDto::Cerrado,
                TipoDiaEspecial::CierreTemprano { .. } => TipoDiaEspecialDto::CierreTemprano,
            },
            hora_cierre: dia.tipo().hora_cierre(),
            automatico: dia.origen() == OrigenDiaEspecial::Automatico,
        }
    }
}

/// Horario de atención de un día de la semana (RF-CFG-03, DA-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HorarioDiaDto {
    /// 1 = lunes … 7 = domingo.
    pub dia_semana: u8,
    pub nombre: &'static str,
    /// `false` si ningún barbero vigente trabaja ese día.
    pub abierto: bool,
    pub abre: Option<NaiveTime>,
    pub cierra: Option<NaiveTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoliticaDto {
    pub id: i64,
    pub tipo: &'static str,
    pub version: String,
    pub url: String,
    pub vigente_desde: DateTime<Utc>,
    /// La versión que rige hoy para su tipo: la más reciente que ya empezó.
    pub vigente: bool,
}

/// Resultado de la precarga de festivos (RF-CFG-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FestivosPrecargadosDto {
    pub anio: i32,
    pub creados: Vec<DiaEspecialDto>,
    /// Fechas que ya tenían un día especial (se respeta lo existente) o que ya pasaron.
    pub omitidos: usize,
}
