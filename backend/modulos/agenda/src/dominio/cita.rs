//! Estados de la cita y sus transiciones (RN-09, RN-11, RN-31).

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EstadoCita {
    SinConfirmar,
    Confirmada,
    EnCurso,
    Atendida,
    NoAsistio,
    Cancelada,
}

impl EstadoCita {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            EstadoCita::SinConfirmar => "sin_confirmar",
            EstadoCita::Confirmada => "confirmada",
            EstadoCita::EnCurso => "en_curso",
            EstadoCita::Atendida => "atendida",
            EstadoCita::NoAsistio => "no_asistio",
            EstadoCita::Cancelada => "cancelada",
        }
    }

    pub(crate) fn desde_codigo(codigo: &str) -> Option<Self> {
        [
            EstadoCita::SinConfirmar,
            EstadoCita::Confirmada,
            EstadoCita::EnCurso,
            EstadoCita::Atendida,
            EstadoCita::NoAsistio,
            EstadoCita::Cancelada,
        ]
        .into_iter()
        .find(|e| e.codigo() == codigo)
    }

    /// Pendiente de atender: se puede cancelar, reprogramar o confirmar.
    pub(crate) fn pendiente(self) -> bool {
        matches!(self, EstadoCita::SinConfirmar | EstadoCita::Confirmada)
    }
}

/// «No asistió» solo el día de la cita o después (RN-11).
pub(crate) fn puede_marcar_inasistencia(estado: EstadoCita, fecha_cita: NaiveDate, hoy: NaiveDate) -> bool {
    estado.pendiente() && fecha_cita <= hoy
}

/// Cancelación con menos anticipación que el aviso mínimo: se marca como tardía, pero siempre
/// es una cancelación y nunca una inasistencia (RN-09, DA-17).
pub(crate) fn cancelacion_tardia(inicio: DateTime<Utc>, ahora: DateTime<Utc>, aviso: TimeDelta) -> bool {
    ahora > inicio - aviso
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn transiciones() {
        assert!(EstadoCita::Confirmada.pendiente() && EstadoCita::SinConfirmar.pendiente());
        assert!(!EstadoCita::EnCurso.pendiente());
        assert_eq!(
            EstadoCita::desde_codigo("no_asistio"),
            Some(EstadoCita::NoAsistio)
        );
        let hoy: NaiveDate = "2026-11-20".parse().unwrap();
        assert!(puede_marcar_inasistencia(EstadoCita::Confirmada, hoy, hoy));
        assert!(
            !puede_marcar_inasistencia(EstadoCita::Confirmada, hoy.succ_opt().unwrap(), hoy),
            "E2E-AGE-05"
        );
        assert!(!puede_marcar_inasistencia(EstadoCita::Cancelada, hoy, hoy));
    }

    #[test]
    fn la_cancelacion_dentro_del_aviso_es_tardia() {
        let inicio: DateTime<Utc> = "2026-11-20T20:00:00Z".parse().unwrap();
        let aviso = TimeDelta::hours(3);
        assert!(
            cancelacion_tardia(inicio, inicio - TimeDelta::hours(2), aviso),
            "E2E-RES-09"
        );
        assert!(!cancelacion_tardia(inicio, inicio - TimeDelta::hours(4), aviso));
    }
}
