use chrono::{DateTime, TimeDelta, Utc};

/// Cuánto dura una sesión sin uso. Cada uso la extiende (expiración deslizante).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DuracionSesion {
    Normal,
    /// «Recordarme» en ese dispositivo.
    Extendida,
}

/// No se escribe en la base en cada petición: solo si el último uso registrado tiene más de
/// este tiempo.
const RENOVAR_CADA: TimeDelta = TimeDelta::minutes(5);

/// Vida máxima de una sesión desde que se inició, aunque se use todos los días: después hay
/// que volver a escribir la contraseña (OWASP ASVS 3.3.2, nivel 1: 30 días).
pub(crate) const VIDA_MAXIMA: TimeDelta = TimeDelta::days(30);

impl DuracionSesion {
    pub(crate) fn desde_recordarme(recordarme: bool) -> Self {
        if recordarme { Self::Extendida } else { Self::Normal }
    }

    pub(crate) fn recordarme(self) -> bool {
        self == Self::Extendida
    }

    pub(crate) fn inactividad_maxima(self) -> TimeDelta {
        match self {
            Self::Normal => TimeDelta::hours(12),
            Self::Extendida => TimeDelta::days(30),
        }
    }

    pub(crate) fn expira_en(self, ultimo_uso: DateTime<Utc>) -> DateTime<Utc> {
        ultimo_uso + self.inactividad_maxima()
    }

    /// Vencimiento al renovarla: la inactividad máxima desde este uso, sin pasar de la vida
    /// máxima desde que se inició.
    pub(crate) fn vence_en(self, ultimo_uso: DateTime<Utc>, creada_en: DateTime<Utc>) -> DateTime<Utc> {
        self.expira_en(ultimo_uso).min(creada_en + VIDA_MAXIMA)
    }

    /// Duración de la cookie: de sesión del navegador, o 30 días con «Recordarme».
    pub(crate) fn duracion_cookie(self) -> Option<std::time::Duration> {
        match self {
            Self::Normal => None,
            Self::Extendida => self.inactividad_maxima().to_std().ok(),
        }
    }

    pub(crate) fn debe_renovarse(ultimo_uso: DateTime<Utc>, ahora: DateTime<Utc>) -> bool {
        ahora - ultimo_uso >= RENOVAR_CADA
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_sesion_normal_dura_12_horas_y_la_extendida_30_dias() {
        let ahora: DateTime<Utc> = "2026-11-20T12:00:00Z".parse().unwrap();
        assert_eq!(
            DuracionSesion::Normal.expira_en(ahora),
            "2026-11-21T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            DuracionSesion::Extendida.expira_en(ahora),
            "2026-12-20T12:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(DuracionSesion::Normal.duracion_cookie(), None);
    }

    #[test]
    fn solo_se_renueva_cada_cinco_minutos() {
        let uso: DateTime<Utc> = "2026-11-20T12:00:00Z".parse().unwrap();
        assert!(!DuracionSesion::debe_renovarse(uso, uso + TimeDelta::minutes(4)));
        assert!(DuracionSesion::debe_renovarse(uso, uso + TimeDelta::minutes(5)));
    }
}
