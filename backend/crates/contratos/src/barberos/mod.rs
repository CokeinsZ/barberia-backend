//! Eventos que publica el módulo `barberos`.

use chrono::{DateTime, NaiveDate, Utc};
use nucleo::identificadores::{IdBarbero, IdUsuario};

use crate::evento;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HorarioCambiado {
    pub barbero: IdBarbero,
}
evento!(HorarioCambiado, "barberos.horario_cambiado");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BloqueoCreado {
    pub barbero: IdBarbero,
    pub inicio: DateTime<Utc>,
    pub fin: DateTime<Utc>,
}
evento!(BloqueoCreado, "barberos.bloqueo_creado");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AusenciaRegistrada {
    pub barbero: IdBarbero,
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
}
evento!(AusenciaRegistrada, "barberos.ausencia_registrada");

/// El barbero se retiró o perdió el acceso al panel: `identidad` desactiva la cuenta y cierra sesiones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccesoRevocado {
    pub barbero: IdBarbero,
    pub usuario: IdUsuario,
}
evento!(AccesoRevocado, "barberos.acceso_revocado");
