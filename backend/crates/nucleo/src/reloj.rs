//! El tiempo es una dependencia: nada llama a `Utc::now()` directamente.
//! Así las pruebas (incluidas las E2E) pueden fijar la hora del sistema.

use std::sync::RwLock;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use chrono_tz::Tz;

/// Zona horaria del negocio (S-01). Las fechas de operación se calculan en esta zona.
pub const ZONA_HORARIA: Tz = chrono_tz::America::Bogota;

pub trait Reloj: Send + Sync + std::fmt::Debug + 'static {
    /// Instante actual en UTC.
    fn ahora(&self) -> DateTime<Utc>;

    /// Instante actual en la hora de Bogotá.
    fn ahora_local(&self) -> DateTime<Tz> {
        self.ahora().with_timezone(&ZONA_HORARIA)
    }

    /// Fecha de hoy en Bogotá.
    fn hoy(&self) -> NaiveDate {
        self.ahora_local().date_naive()
    }
}

/// Reloj real del sistema.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelojSistema;

impl Reloj for RelojSistema {
    fn ahora(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Reloj que se puede fijar y adelantar (pruebas y ambiente E2E).
#[derive(Debug)]
pub struct RelojControlado {
    instante: RwLock<DateTime<Utc>>,
}

impl RelojControlado {
    pub fn en(instante: DateTime<Utc>) -> Self {
        Self {
            instante: RwLock::new(instante),
        }
    }

    pub fn fijar(&self, instante: DateTime<Utc>) {
        *self.instante.write().unwrap_or_else(|e| e.into_inner()) = instante;
    }

    pub fn adelantar(&self, delta: TimeDelta) {
        let mut instante = self.instante.write().unwrap_or_else(|e| e.into_inner());
        *instante += delta;
    }
}

impl Reloj for RelojControlado {
    fn ahora(&self) -> DateTime<Utc> {
        *self.instante.read().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_fecha_de_hoy_se_calcula_en_bogota() {
        // 03:30 UTC del 20 de noviembre son las 10:30 p. m. del 19 en Bogotá.
        let reloj = RelojControlado::en("2026-11-20T03:30:00Z".parse().unwrap());
        assert_eq!(reloj.hoy(), NaiveDate::from_ymd_opt(2026, 11, 19).unwrap());
        reloj.adelantar(TimeDelta::hours(2));
        assert_eq!(reloj.hoy(), NaiveDate::from_ymd_opt(2026, 11, 20).unwrap());
    }
}
