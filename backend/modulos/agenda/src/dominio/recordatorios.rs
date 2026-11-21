//! Cuándo se envía cada recordatorio automático (RF-NOT-02).
//!
//! - El de **24 h** se envía desde 24 h antes de la cita y hasta 2 h antes; si para entonces no
//!   salió (el worker estuvo detenido), ya no se envía: queda el de 2 h.
//! - El de **2 h** se envía desde 2 h antes y hasta la hora de la cita.
//! - Un recordatorio solo tiene sentido si la cita ya tenía su hora cuando el recordatorio
//!   «venció»: una cita agendada hace una hora para dentro de tres no recibe el de 24 h (acaba de
//!   recibir la confirmación), pero sí el de 2 h.
//! - Cada recordatorio sale una sola vez por hora de la cita (si se reprograma, la hora nueva
//!   tiene sus propios recordatorios). Eso lo garantiza además un índice único en la base.

use chrono::{DateTime, TimeDelta, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recordatorio {
    Dia,
    DosHoras,
}

impl Recordatorio {
    /// Valor del enum `agenda.proposito_mensaje`.
    pub(crate) fn proposito(self) -> &'static str {
        match self {
            Recordatorio::Dia => "recordatorio_24h",
            Recordatorio::DosHoras => "recordatorio_2h",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReglaRecordatorios {
    pub(crate) con_24h: bool,
    pub(crate) con_2h: bool,
}

/// El recordatorio que corresponde enviar ahora, si alguno. `enviados` son los propósitos ya
/// enviados para la hora actual de la cita.
pub(crate) fn recordatorio_debido(
    regla: ReglaRecordatorios,
    inicio: DateTime<Utc>,
    vigente_desde: DateTime<Utc>,
    ahora: DateTime<Utc>,
    enviados: &[String],
) -> Option<Recordatorio> {
    if ahora >= inicio {
        return None;
    }
    let ya = |r: Recordatorio| enviados.iter().any(|e| e == r.proposito());
    let dos_horas = inicio - TimeDelta::hours(2);
    let un_dia = inicio - TimeDelta::hours(24);
    if ahora >= dos_horas {
        return (regla.con_2h && vigente_desde <= dos_horas && !ya(Recordatorio::DosHoras))
            .then_some(Recordatorio::DosHoras);
    }
    (regla.con_24h && ahora >= un_dia && vigente_desde <= un_dia && !ya(Recordatorio::Dia))
        .then_some(Recordatorio::Dia)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const AMBOS: ReglaRecordatorios = ReglaRecordatorios {
        con_24h: true,
        con_2h: true,
    };

    fn t(texto: &str) -> DateTime<Utc> {
        texto.parse().unwrap()
    }

    /// Cita el jueves 26 a las 3:00 p. m. de Bogotá (20:00 UTC), agendada con días de anticipación.
    const INICIO: &str = "2026-11-26T20:00:00Z";
    const AGENDADA: &str = "2026-11-20T15:00:00Z";

    #[test]
    fn el_de_24_horas_sale_desde_un_dia_antes_y_una_sola_vez() {
        let debido = |ahora: &str, enviados: &[String]| {
            recordatorio_debido(AMBOS, t(INICIO), t(AGENDADA), t(ahora), enviados)
        };
        assert_eq!(debido("2026-11-25T19:59:59Z", &[]), None, "todavía no");
        assert_eq!(
            debido("2026-11-25T20:00:00Z", &[]),
            Some(Recordatorio::Dia),
            "E2E-NOT-01"
        );
        assert_eq!(debido("2026-11-26T10:00:00Z", &[]), Some(Recordatorio::Dia));
        assert_eq!(
            debido("2026-11-26T10:00:00Z", &["recordatorio_24h".into()]),
            None,
            "ya salió"
        );
    }

    #[test]
    fn el_de_2_horas_reemplaza_al_de_24_si_este_no_alcanzo_a_salir() {
        let debido = |ahora: &str| recordatorio_debido(AMBOS, t(INICIO), t(AGENDADA), t(ahora), &[]);
        assert_eq!(debido("2026-11-26T18:00:00Z"), Some(Recordatorio::DosHoras));
        assert_eq!(debido("2026-11-26T19:59:00Z"), Some(Recordatorio::DosHoras));
        assert_eq!(debido("2026-11-26T20:00:00Z"), None, "la cita ya empezó");
    }

    #[test]
    fn una_cita_agendada_tarde_no_recibe_los_recordatorios_ya_vencidos() {
        // Agendada 5 horas antes: sin el de 24 h, con el de 2 h.
        let agendada = t("2026-11-26T15:00:00Z");
        assert_eq!(
            recordatorio_debido(AMBOS, t(INICIO), agendada, t("2026-11-26T15:05:00Z"), &[]),
            None
        );
        assert_eq!(
            recordatorio_debido(AMBOS, t(INICIO), agendada, t("2026-11-26T18:00:00Z"), &[]),
            Some(Recordatorio::DosHoras)
        );
        // Agendada una hora antes: ninguno (acaba de recibir la confirmación).
        assert_eq!(
            recordatorio_debido(
                AMBOS,
                t(INICIO),
                t("2026-11-26T19:00:00Z"),
                t("2026-11-26T19:10:00Z"),
                &[]
            ),
            None
        );
    }

    #[test]
    fn respeta_la_configuracion() {
        let solo_24 = ReglaRecordatorios {
            con_24h: true,
            con_2h: false,
        };
        let ninguno = ReglaRecordatorios {
            con_24h: false,
            con_2h: false,
        };
        assert_eq!(
            recordatorio_debido(solo_24, t(INICIO), t(AGENDADA), t("2026-11-26T18:30:00Z"), &[]),
            None
        );
        assert_eq!(
            recordatorio_debido(solo_24, t(INICIO), t(AGENDADA), t("2026-11-26T08:00:00Z"), &[]),
            Some(Recordatorio::Dia)
        );
        assert_eq!(
            recordatorio_debido(ninguno, t(INICIO), t(AGENDADA), t("2026-11-26T08:00:00Z"), &[]),
            None
        );
    }
}
