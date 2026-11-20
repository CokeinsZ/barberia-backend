use chrono::{DateTime, TimeDelta, Utc};

/// Intentos máximos antes de dar un mensaje por fallido.
pub(crate) const MAX_INTENTOS: i16 = 5;

/// Próximo intento con espera exponencial (1, 2, 4, 8 minutos…, máximo 1 hora), o `None` si
/// ya se agotaron los intentos.
pub(crate) fn proximo_intento(intentos_hechos: i16, ahora: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if intentos_hechos >= MAX_INTENTOS {
        return None;
    }
    let exponente = u32::try_from(intentos_hechos.max(1) - 1).unwrap_or(0);
    let minutos = 2_i64.saturating_pow(exponente).min(60);
    Some(ahora + TimeDelta::minutes(minutos))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn espera_cada_vez_mas_y_luego_se_rinde() {
        let ahora: DateTime<Utc> = "2026-11-20T12:00:00Z".parse().unwrap();
        let minutos: Vec<i64> = (1..MAX_INTENTOS)
            .map(|n| (proximo_intento(n, ahora).unwrap() - ahora).num_minutes())
            .collect();
        assert_eq!(minutos, [1, 2, 4, 8]);
        assert_eq!(proximo_intento(MAX_INTENTOS, ahora), None);
    }
}
