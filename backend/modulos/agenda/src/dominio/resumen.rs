//! Tiempo de un día de trabajo (RF-PBA-02, RN-21): minutos disponibles (las franjas menos los
//! bloqueos), ocupados (citas vigentes dentro de lo disponible) y libres.

use chrono::{DateTime, NaiveTime, Timelike, Utc};

/// Minutos desde la medianoche de Bogotá: `[inicio, fin)`.
pub(crate) type Intervalo = (u32, u32);

const MINUTOS_DEL_DIA: i64 = 24 * 60;

pub(crate) fn minutos(hora: NaiveTime) -> u32 {
    hora.hour() * 60 + hora.minute()
}

/// La parte de `[inicio, fin)` que cae en el día que empieza en `medianoche` (Colombia no tiene
/// horario de verano: el día siempre tiene 1.440 minutos).
pub(crate) fn en_el_dia(
    medianoche: DateTime<Utc>,
    inicio: DateTime<Utc>,
    fin: DateTime<Utc>,
) -> Option<Intervalo> {
    let desde = (inicio - medianoche).num_minutes().clamp(0, MINUTOS_DEL_DIA);
    let hasta = (fin - medianoche).num_minutes().clamp(0, MINUTOS_DEL_DIA);
    (hasta > desde).then(|| {
        (
            u32::try_from(desde).unwrap_or_default(),
            u32::try_from(hasta).unwrap_or_default(),
        )
    })
}

fn traslape(a: Intervalo, b: Intervalo) -> Option<Intervalo> {
    let (inicio, fin) = (a.0.max(b.0), a.1.min(b.1));
    (fin > inicio).then_some((inicio, fin))
}

fn largo(intervalo: Option<Intervalo>) -> u32 {
    intervalo.map_or(0, |(inicio, fin)| fin - inicio)
}

/// Minutos de `intervalo` que caen en alguna franja (las franjas de un día no se cruzan).
fn dentro(intervalo: Intervalo, franjas: &[Intervalo]) -> u32 {
    franjas.iter().map(|f| largo(traslape(intervalo, *f))).sum()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MinutosDelDia {
    /// Las franjas del día menos lo bloqueado.
    pub(crate) disponibles: u32,
    /// Lo que las citas vigentes ocupan de lo disponible.
    pub(crate) ocupados: u32,
}

impl MinutosDelDia {
    pub(crate) fn libres(self) -> u32 {
        self.disponibles.saturating_sub(self.ocupados)
    }
}

/// Disponibles = franjas − bloqueos; ocupados = citas dentro de las franjas y fuera de los
/// bloqueos. Una cita que quedó dentro de un bloqueo (se bloqueó después de agendarla) no
/// cuenta dos veces: esos minutos ya no estaban disponibles. Los bloqueos de un barbero no se
/// cruzan entre sí, ni sus citas vigentes (restricciones de la base).
pub(crate) fn minutos_del_dia(
    franjas: &[Intervalo],
    bloqueos: &[Intervalo],
    citas: &[Intervalo],
) -> MinutosDelDia {
    let total: u32 = franjas.iter().map(|(inicio, fin)| fin - inicio).sum();
    let bloqueado: u32 = bloqueos.iter().map(|b| dentro(*b, franjas)).sum();
    let ocupados: u32 = citas
        .iter()
        .map(|c| {
            let en_bloqueo: u32 = bloqueos
                .iter()
                .filter_map(|b| traslape(*c, *b))
                .map(|t| dentro(t, franjas))
                .sum();
            dentro(*c, franjas).saturating_sub(en_bloqueo)
        })
        .sum();
    let disponibles = total.saturating_sub(bloqueado);
    MinutosDelDia {
        disponibles,
        ocupados: ocupados.min(disponibles),
    }
}

#[cfg(test)]
mod pruebas {
    use chrono::{NaiveDate, TimeZone};

    use super::*;

    fn h(texto: &str) -> u32 {
        minutos(NaiveTime::parse_from_str(texto, "%H:%M").unwrap())
    }

    fn i(desde: &str, hasta: &str) -> Intervalo {
        (h(desde), h(hasta))
    }

    #[test]
    fn franjas_menos_bloqueos_y_citas_dentro_de_lo_disponible() {
        // 9:00–13:00 y 14:00–19:00: 9 horas.
        let franjas = [i("09:00", "13:00"), i("14:00", "19:00")];
        assert_eq!(
            minutos_del_dia(&franjas, &[], &[]),
            MinutosDelDia {
                disponibles: 540,
                ocupados: 0
            }
        );
        // Un bloqueo de una hora y otro que empieza en el almuerzo: solo cuenta lo que cae en franja.
        let bloqueos = [i("16:00", "17:00"), i("12:30", "14:30")];
        // Una cita normal, una que cruza el almuerzo y una que quedó dentro de un bloqueo.
        let citas = [i("09:00", "09:30"), i("12:45", "14:15"), i("16:00", "16:30")];
        let m = minutos_del_dia(&franjas, &bloqueos, &citas);
        assert_eq!(
            m.disponibles,
            540 - 60 - 60,
            "el segundo bloqueo quita 30 + 30 minutos"
        );
        // 30 de la primera; la segunda cae entera en el bloqueo del almuerzo; la tercera, en el
        // de las 16:00.
        assert_eq!(m.ocupados, 30);
        assert_eq!(m.libres(), 420 - 30);
    }

    #[test]
    fn nunca_ocupa_mas_de_lo_disponible() {
        let franjas = [i("09:00", "10:00")];
        let citas = [i("08:00", "11:00")];
        let m = minutos_del_dia(&franjas, &[], &citas);
        assert_eq!((m.disponibles, m.ocupados, m.libres()), (60, 60, 0));
        assert_eq!(
            minutos_del_dia(&[], &[i("09:00", "10:00")], &citas),
            MinutosDelDia::default()
        );
    }

    #[test]
    fn recorta_los_instantes_al_dia() {
        let bogota = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
        let fecha = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let medianoche = bogota
            .from_local_datetime(&fecha.and_hms_opt(0, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let a = |dia: u32, hora: u32, minuto: u32| {
            bogota
                .with_ymd_and_hms(2026, 11, dia, hora, minuto, 0)
                .unwrap()
                .with_timezone(&Utc)
        };
        assert_eq!(
            en_el_dia(medianoche, a(20, 10, 0), a(20, 10, 45)),
            Some((600, 645))
        );
        // Un bloqueo de varios días cubre el día entero.
        assert_eq!(en_el_dia(medianoche, a(19, 8, 0), a(22, 8, 0)), Some((0, 1440)));
        assert_eq!(en_el_dia(medianoche, a(19, 23, 0), a(20, 1, 0)), Some((0, 60)));
        assert_eq!(en_el_dia(medianoche, a(21, 0, 0), a(21, 1, 0)), None);
        assert_eq!(en_el_dia(medianoche, a(19, 22, 0), a(20, 0, 0)), None);
    }
}
