//! Reglas de agendamiento editables (RF-CFG-02) con las opciones del mockup.

use nucleo::{ErrorDeCampo, Validacion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfiguracionAgenda {
    /// 30, 60 o 120 minutos.
    pub(crate) anticipacion_minima_min: i16,
    /// 15, 30 o 60 días.
    pub(crate) ventana_reserva_dias: i16,
    /// 15 o 30 minutos.
    pub(crate) intervalo_min: i16,
    /// 60, 180 o 1440 minutos (1, 3 o 24 horas).
    pub(crate) aviso_cancelacion_min: i16,
    pub(crate) recordatorio_24h: bool,
    pub(crate) recordatorio_2h: bool,
    /// `confirmar_por_llamada`, `bloquear_reserva_en_linea` o `sin_restriccion` (RN-10).
    pub(crate) politica_inasistencias: String,
    pub(crate) umbral_inasistencias: i16,
}

impl ConfiguracionAgenda {
    pub(crate) fn validar(&self) -> Result<(), Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        if ![30, 60, 120].contains(&self.anticipacion_minima_min) {
            v.error("anticipacion_minima_min", "Elige 30, 60 o 120 minutos.");
        }
        if ![15, 30, 60].contains(&self.ventana_reserva_dias) {
            v.error("ventana_reserva_dias", "Elige 15, 30 o 60 días.");
        }
        if ![15, 30].contains(&self.intervalo_min) {
            v.error("intervalo_min", "Elige 15 o 30 minutos.");
        }
        if ![60, 180, 1440].contains(&self.aviso_cancelacion_min) {
            v.error("aviso_cancelacion_min", "Elige 1, 3 o 24 horas.");
        }
        if ![
            "confirmar_por_llamada",
            "bloquear_reserva_en_linea",
            "sin_restriccion",
        ]
        .contains(&self.politica_inasistencias.as_str())
        {
            v.error(
                "politica_inasistencias",
                "Elige confirmar por llamada, bloquear la reserva en línea o sin restricción.",
            );
        }
        if !(1..=10).contains(&self.umbral_inasistencias) {
            v.error("umbral_inasistencias", "El umbral va de 1 a 10 inasistencias.");
        }
        v.terminar(|| ())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_admite_las_opciones_del_panel() {
        let base = ConfiguracionAgenda {
            anticipacion_minima_min: 120,
            ventana_reserva_dias: 30,
            intervalo_min: 30,
            aviso_cancelacion_min: 180,
            recordatorio_24h: true,
            recordatorio_2h: true,
            politica_inasistencias: "confirmar_por_llamada".into(),
            umbral_inasistencias: 2,
        };
        assert!(base.validar().is_ok());
        let mala = ConfiguracionAgenda {
            intervalo_min: 20,
            politica_inasistencias: "otra".into(),
            ..base
        };
        assert_eq!(mala.validar().unwrap_err().len(), 2);
    }
}
