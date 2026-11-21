//! Reglas de la cola: franjas de las tareas periódicas y espera entre reintentos.

use chrono::{DateTime, Days, TimeDelta, Utc};
use nucleo::ZONA_HORARIA;

use crate::api::Periodicidad;

/// Inicio de la franja vigente en `ahora`. Una tarea se programa una sola vez por franja: la
/// franja forma parte de su clave única, así varios workers (o dos pasadas seguidas) no la
/// duplican. Si el worker estuvo detenido, solo se recupera la franja vigente, no las perdidas.
pub(crate) fn franja_vigente(periodicidad: Periodicidad, ahora: DateTime<Utc>) -> DateTime<Utc> {
    match periodicidad {
        Periodicidad::Cada(cada) => {
            let segundos = cada.num_seconds().max(1);
            let inicio = ahora.timestamp().div_euclid(segundos) * segundos;
            DateTime::from_timestamp(inicio, 0).unwrap_or(ahora)
        }
        Periodicidad::DiariaA(hora) => {
            let local = ahora.with_timezone(&ZONA_HORARIA);
            let hoy = local.date_naive();
            let a_la_hora = |fecha: chrono::NaiveDate| {
                fecha
                    .and_time(hora)
                    .and_local_timezone(ZONA_HORARIA)
                    .earliest()
                    .map(|d| d.with_timezone(&Utc))
            };
            match a_la_hora(hoy) {
                Some(instante) if instante <= ahora => instante,
                _ => hoy
                    .checked_sub_days(Days::new(1))
                    .and_then(a_la_hora)
                    .unwrap_or(ahora),
            }
        }
    }
}

/// Clave de idempotencia: `agenda.recordatorios@2026-11-21T15:05:00Z`.
pub(crate) fn clave(tipo: &str, franja: DateTime<Utc>) -> String {
    format!("{tipo}@{}", franja.format("%Y-%m-%dT%H:%M:%SZ"))
}

/// Espera antes de reintentar una tarea que falló: 1, 5, 15 y 60 minutos.
pub(crate) fn espera_reintento(intentos_hechos: i16) -> TimeDelta {
    match intentos_hechos {
        ..=1 => TimeDelta::minutes(1),
        2 => TimeDelta::minutes(5),
        3 => TimeDelta::minutes(15),
        _ => TimeDelta::minutes(60),
    }
}

#[cfg(test)]
mod pruebas {
    use chrono::NaiveTime;

    use super::*;

    fn instante(texto: &str) -> DateTime<Utc> {
        texto.parse().unwrap()
    }

    #[test]
    fn las_franjas_periodicas_se_alinean_al_reloj() {
        let cada5 = Periodicidad::Cada(TimeDelta::minutes(5));
        assert_eq!(
            franja_vigente(cada5, instante("2026-11-21T15:07:59Z")),
            instante("2026-11-21T15:05:00Z")
        );
        assert_eq!(
            franja_vigente(cada5, instante("2026-11-21T15:10:00Z")),
            instante("2026-11-21T15:10:00Z")
        );
        assert_eq!(
            clave("agenda.recordatorios", instante("2026-11-21T15:05:00Z")),
            "agenda.recordatorios@2026-11-21T15:05:00Z"
        );
    }

    #[test]
    fn la_tarea_diaria_usa_la_hora_de_bogota() {
        let tres = Periodicidad::DiariaA(NaiveTime::from_hms_opt(3, 0, 0).unwrap());
        // 2:30 a. m. en Bogotá (07:30 UTC): todavía rige la de ayer a las 3:00 a. m.
        assert_eq!(
            franja_vigente(tres, instante("2026-11-21T07:30:00Z")),
            instante("2026-11-20T08:00:00Z")
        );
        // 10:00 a. m. en Bogotá: la de hoy a las 3:00 a. m. (08:00 UTC).
        assert_eq!(
            franja_vigente(tres, instante("2026-11-21T15:00:00Z")),
            instante("2026-11-21T08:00:00Z")
        );
    }

    #[test]
    fn los_reintentos_esperan_cada_vez_mas() {
        let minutos: Vec<i64> = (1..=5).map(|n| espera_reintento(n).num_minutes()).collect();
        assert_eq!(minutos, [1, 5, 15, 60, 60]);
    }
}
