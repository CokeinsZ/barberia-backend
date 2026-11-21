//! Motor de disponibilidad (RF-SIS-01, RF-SIS-02, RN-01 a RN-06).
//!
//! Función pura: con las franjas, ausencias, retiro, bloqueos y citas de cada barbero, los días
//! especiales y las reglas de agendamiento, calcula las horas de inicio libres. Una hora se
//! ofrece solo si el servicio completo cabe en una franja y no se cruza con nada ocupado.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeDelta, Utc};
use nucleo::ZONA_HORARIA;
use nucleo::identificadores::IdBarbero;

/// Reglas configurables (RF-CFG-02, valores por defecto de RN-01 y RN-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reglas {
    pub(crate) anticipacion: TimeDelta,
    pub(crate) ventana_dias: i64,
    pub(crate) intervalo: TimeDelta,
}

/// Cómo se trata cada fecha (RN-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoDia {
    Normal,
    /// Cada barbero trabaja con su horario de domingo.
    Festivo,
    Cerrado,
    /// Las horas que terminan después de la hora de cierre no se ofrecen.
    CierreTemprano(NaiveTime),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Franja {
    /// 1 = lunes … 7 = domingo.
    pub(crate) dia_semana: u8,
    pub(crate) inicio: NaiveTime,
    pub(crate) fin: NaiveTime,
}

/// Un barbero tal como lo ve el motor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Agendable {
    pub(crate) id: IdBarbero,
    pub(crate) franjas: Vec<Franja>,
    /// Ausencias vigentes (fechas incluidas): en esas fechas no trabaja.
    pub(crate) ausencias: Vec<(NaiveDate, NaiveDate)>,
    /// Desde esta fecha no trabaja (retiro).
    pub(crate) fecha_retiro: Option<NaiveDate>,
    /// Citas activas y bloqueos.
    pub(crate) ocupado: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl Agendable {
    /// Trabaja ese día: no se ha retirado ni tiene una ausencia que lo cubra (RN-24).
    pub(crate) fn trabaja(&self, fecha: NaiveDate) -> bool {
        self.fecha_retiro.is_none_or(|retiro| fecha < retiro)
            && !self
                .ausencias
                .iter()
                .any(|(desde, hasta)| (*desde..=*hasta).contains(&fecha))
    }
}

/// Quién consulta: el sitio público aplica la anticipación mínima y la ventana de reserva; el
/// personal puede agendar «ahora mismo» (RF-AGE-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modo {
    Publico,
    Personal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Consulta {
    pub(crate) ahora: DateTime<Utc>,
    pub(crate) duracion: TimeDelta,
    pub(crate) reglas: Reglas,
    pub(crate) modo: Modo,
}

impl Consulta {
    pub(crate) fn hoy(&self) -> NaiveDate {
        self.ahora.with_timezone(&ZONA_HORARIA).date_naive()
    }

    /// Fechas que se pueden ofrecer: desde hoy y, en el sitio público, dentro de la ventana.
    pub(crate) fn fecha_permitida(&self, fecha: NaiveDate) -> bool {
        let hoy = self.hoy();
        match self.modo {
            Modo::Publico => fecha >= hoy && (fecha - hoy).num_days() <= self.reglas.ventana_dias,
            Modo::Personal => fecha >= hoy,
        }
    }

    fn inicio_permitido(&self, inicio: DateTime<Utc>) -> bool {
        match self.modo {
            Modo::Publico => inicio >= self.ahora + self.reglas.anticipacion,
            // El personal puede usar el turno que está corriendo (cliente que llega sin cita).
            Modo::Personal => inicio + self.reglas.intervalo > self.ahora,
        }
    }
}

fn a_utc(fecha: NaiveDate, hora: NaiveTime) -> Option<DateTime<Utc>> {
    fecha
        .and_time(hora)
        .and_local_timezone(ZONA_HORARIA)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
}

/// Horas de inicio libres de un barbero en una fecha, en orden.
pub(crate) fn horas_del_barbero(
    barbero: &Agendable,
    fecha: NaiveDate,
    tipo: TipoDia,
    c: &Consulta,
) -> Vec<DateTime<Utc>> {
    if !c.fecha_permitida(fecha) || !barbero.trabaja(fecha) || c.reglas.intervalo <= TimeDelta::zero() {
        return Vec::new();
    }
    let (dia, cierre) = match tipo {
        TipoDia::Cerrado => return Vec::new(),
        TipoDia::Festivo => (7, None),
        TipoDia::CierreTemprano(hora) => (fecha.weekday().number_from_monday(), Some(hora)),
        TipoDia::Normal => (fecha.weekday().number_from_monday(), None),
    };
    let mut horas = Vec::new();
    for franja in barbero.franjas.iter().filter(|f| u32::from(f.dia_semana) == dia) {
        let mut inicio_local = franja.inicio;
        loop {
            let (fin_local, desborde) = inicio_local.overflowing_add_signed(c.duracion);
            if desborde != 0 || fin_local > franja.fin || cierre.is_some_and(|h| fin_local > h) {
                break;
            }
            if let (Some(inicio), Some(fin)) = (a_utc(fecha, inicio_local), a_utc(fecha, fin_local)) {
                let libre = !barbero
                    .ocupado
                    .iter()
                    .any(|(o_inicio, o_fin)| inicio < *o_fin && fin > *o_inicio);
                if libre && c.inicio_permitido(inicio) {
                    horas.push(inicio);
                }
            }
            let (siguiente, desborde) = inicio_local.overflowing_add_signed(c.reglas.intervalo);
            if desborde != 0 {
                break;
            }
            inicio_local = siguiente;
        }
    }
    horas.sort();
    horas.dedup();
    horas
}

/// Horas libres de varios barberos (ya en el orden del equipo). Cada hora trae el primer barbero
/// libre en ese orden: así se asigna «Cualquier barbero» (RN-06, DA-03).
pub(crate) fn horas(
    barberos: &[Agendable],
    fecha: NaiveDate,
    tipo: TipoDia,
    c: &Consulta,
) -> Vec<(DateTime<Utc>, IdBarbero)> {
    let mut por_hora: BTreeMap<DateTime<Utc>, IdBarbero> = BTreeMap::new();
    for barbero in barberos {
        for hora in horas_del_barbero(barbero, fecha, tipo, c) {
            por_hora.entry(hora).or_insert(barbero.id);
        }
    }
    por_hora.into_iter().collect()
}

/// Estado de un día en el calendario (RF-RES-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EstadoDia {
    Disponible,
    /// 3 horas libres o menos.
    UltimosCupos,
    SinCupos,
    FueraDeVentana,
}

impl EstadoDia {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            EstadoDia::Disponible => "disponible",
            EstadoDia::UltimosCupos => "ultimos_cupos",
            EstadoDia::SinCupos => "sin_cupos",
            EstadoDia::FueraDeVentana => "fuera_de_ventana",
        }
    }

    pub(crate) fn por_cantidad(horas: usize) -> Self {
        match horas {
            0 => EstadoDia::SinCupos,
            1..=3 => EstadoDia::UltimosCupos,
            _ => EstadoDia::Disponible,
        }
    }
}

/// Franja del día del tablero (con festivo y cierre temprano aplicados).
pub(crate) fn franjas_del_dia(
    barbero: &Agendable,
    fecha: NaiveDate,
    tipo: TipoDia,
) -> Vec<(NaiveTime, NaiveTime)> {
    if !barbero.trabaja(fecha) {
        return Vec::new();
    }
    let (dia, cierre) = match tipo {
        TipoDia::Cerrado => return Vec::new(),
        TipoDia::Festivo => (7, None),
        TipoDia::CierreTemprano(hora) => (fecha.weekday().number_from_monday(), Some(hora)),
        TipoDia::Normal => (fecha.weekday().number_from_monday(), None),
    };
    barbero
        .franjas
        .iter()
        .filter(|f| u32::from(f.dia_semana) == dia)
        .filter_map(|f| {
            let fin = cierre.map_or(f.fin, |h| f.fin.min(h));
            (fin > f.inicio).then_some((f.inicio, fin))
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn d(t: &str) -> NaiveDate {
        t.parse().unwrap()
    }

    fn h(t: &str) -> NaiveTime {
        NaiveTime::parse_from_str(t, "%H:%M").unwrap()
    }

    /// Instante a partir de una fecha y hora de Bogotá.
    fn bogota(fecha: &str, hora: &str) -> DateTime<Utc> {
        a_utc(d(fecha), h(hora)).unwrap()
    }

    fn reglas() -> Reglas {
        Reglas {
            anticipacion: TimeDelta::hours(2),
            ventana_dias: 30,
            intervalo: TimeDelta::minutes(30),
        }
    }

    fn consulta(ahora: DateTime<Utc>, minutos: i64, modo: Modo) -> Consulta {
        Consulta {
            ahora,
            duracion: TimeDelta::minutes(minutos),
            reglas: reglas(),
            modo,
        }
    }

    /// Lunes a sábado de 9:00 a 13:00 y de 14:00 a 19:00; domingo de 10:00 a 14:00.
    fn santiago() -> Agendable {
        let mut franjas: Vec<Franja> = (1..=6)
            .flat_map(|dia| {
                [
                    Franja {
                        dia_semana: dia,
                        inicio: h("09:00"),
                        fin: h("13:00"),
                    },
                    Franja {
                        dia_semana: dia,
                        inicio: h("14:00"),
                        fin: h("19:00"),
                    },
                ]
            })
            .collect();
        franjas.push(Franja {
            dia_semana: 7,
            inicio: h("10:00"),
            fin: h("14:00"),
        });
        Agendable {
            id: IdBarbero(1),
            franjas,
            ausencias: Vec::new(),
            fecha_retiro: None,
            ocupado: Vec::new(),
        }
    }

    fn texto(horas: &[DateTime<Utc>]) -> Vec<String> {
        horas
            .iter()
            .map(|h| h.with_timezone(&ZONA_HORARIA).format("%H:%M").to_string())
            .collect()
    }

    // Jueves 19 de noviembre de 2026; el «ahora» es el miércoles 18 a las 2:50 p. m.
    const JUEVES: &str = "2026-11-19";
    fn ahora() -> DateTime<Utc> {
        bogota("2026-11-18", "14:50")
    }

    #[test]
    fn ofrece_cada_30_minutos_si_el_servicio_cabe_en_la_franja() {
        let horas = horas_del_barbero(
            &santiago(),
            d(JUEVES),
            TipoDia::Normal,
            &consulta(ahora(), 45, Modo::Publico),
        );
        assert_eq!(
            texto(&horas),
            [
                "09:00", "09:30", "10:00", "10:30", "11:00", "11:30", "12:00", "14:00", "14:30", "15:00",
                "15:30", "16:00", "16:30", "17:00", "17:30", "18:00"
            ],
            "12:30 + 45 min se sale de la franja; 18:30 también"
        );
    }

    #[test]
    fn el_mismo_dia_exige_la_anticipacion_minima() {
        // E2E-RES-04: a las 2:50 p. m., la primera hora es a partir de las 4:50 p. m.
        let horas = horas_del_barbero(
            &santiago(),
            d("2026-11-18"),
            TipoDia::Normal,
            &consulta(ahora(), 30, Modo::Publico),
        );
        assert_eq!(texto(&horas).first().map(String::as_str), Some("17:00"));
        // El personal puede agendar en el turno que está corriendo.
        let personal = horas_del_barbero(
            &santiago(),
            d("2026-11-18"),
            TipoDia::Normal,
            &consulta(ahora(), 30, Modo::Personal),
        );
        assert_eq!(texto(&personal).first().map(String::as_str), Some("14:30"));
        // Nunca días pasados.
        assert!(
            horas_del_barbero(
                &santiago(),
                d("2026-11-17"),
                TipoDia::Normal,
                &consulta(ahora(), 30, Modo::Personal)
            )
            .is_empty()
        );
    }

    #[test]
    fn respeta_la_ventana_de_reserva() {
        let c = consulta(ahora(), 30, Modo::Publico);
        assert!(
            !horas_del_barbero(&santiago(), d("2026-12-18"), TipoDia::Normal, &c).is_empty(),
            "día 30"
        );
        assert!(
            horas_del_barbero(&santiago(), d("2026-12-19"), TipoDia::Normal, &c).is_empty(),
            "día 31"
        );
        assert!(
            !horas_del_barbero(
                &santiago(),
                d("2027-02-01"),
                TipoDia::Normal,
                &consulta(ahora(), 30, Modo::Personal)
            )
            .is_empty()
        );
    }

    #[test]
    fn festivo_usa_el_domingo_cerrado_no_ofrece_y_cierre_temprano_recorta() {
        let c = consulta(ahora(), 30, Modo::Publico);
        assert_eq!(
            texto(&horas_del_barbero(&santiago(), d(JUEVES), TipoDia::Festivo, &c)),
            [
                "10:00", "10:30", "11:00", "11:30", "12:00", "12:30", "13:00", "13:30"
            ]
        );
        assert!(horas_del_barbero(&santiago(), d(JUEVES), TipoDia::Cerrado, &c).is_empty());
        // E2E-CFG-01: cierre temprano a las 2:00 p. m.: nada termina después.
        let temprano = horas_del_barbero(&santiago(), d(JUEVES), TipoDia::CierreTemprano(h("14:00")), &c);
        assert_eq!(texto(&temprano).last().map(String::as_str), Some("12:30"));
        assert_eq!(
            franjas_del_dia(&santiago(), d(JUEVES), TipoDia::CierreTemprano(h("11:00"))),
            [(h("09:00"), h("11:00"))]
        );
    }

    #[test]
    fn ausencias_y_retiro_dejan_al_barbero_sin_horas() {
        let c = consulta(ahora(), 30, Modo::Publico);
        let vacaciones = Agendable {
            ausencias: vec![(d("2026-11-19"), d("2026-11-20"))],
            ..santiago()
        };
        assert!(horas_del_barbero(&vacaciones, d(JUEVES), TipoDia::Normal, &c).is_empty());
        assert!(
            !horas_del_barbero(&vacaciones, d("2026-11-21"), TipoDia::Normal, &c).is_empty(),
            "al terminar vuelve solo"
        );
        let retirado = Agendable {
            fecha_retiro: Some(d(JUEVES)),
            ..santiago()
        };
        assert!(horas_del_barbero(&retirado, d(JUEVES), TipoDia::Normal, &c).is_empty());
        assert!(
            !horas_del_barbero(
                &retirado,
                d("2026-11-18"),
                TipoDia::Normal,
                &consulta(ahora(), 30, Modo::Personal)
            )
            .is_empty()
        );
    }

    #[test]
    fn citas_y_bloqueos_quitan_las_horas_que_se_cruzan() {
        // Cita de 15:00 a 15:45 y bloqueo de 17:00 a 18:00 (E2E-BAR-06).
        let ocupado = Agendable {
            ocupado: vec![
                (bogota(JUEVES, "15:00"), bogota(JUEVES, "15:45")),
                (bogota(JUEVES, "17:00"), bogota(JUEVES, "18:00")),
            ],
            ..santiago()
        };
        let tarde: Vec<String> = texto(&horas_del_barbero(
            &ocupado,
            d(JUEVES),
            TipoDia::Normal,
            &consulta(ahora(), 30, Modo::Publico),
        ))
        .into_iter()
        .filter(|h| h.as_str() >= "14:00")
        .collect();
        assert_eq!(
            tarde,
            ["14:00", "14:30", "16:00", "16:30", "18:00", "18:30"],
            "15:45 libera desde las 16:00; lo adyacente sí se ofrece"
        );
    }

    #[test]
    fn cualquier_barbero_asigna_al_primero_libre_segun_el_orden() {
        let primero = Agendable {
            ocupado: vec![(bogota(JUEVES, "09:00"), bogota(JUEVES, "10:00"))],
            ..santiago()
        };
        let segundo = Agendable {
            id: IdBarbero(2),
            ..santiago()
        };
        let resultado = horas(
            &[primero, segundo],
            d(JUEVES),
            TipoDia::Normal,
            &consulta(ahora(), 30, Modo::Publico),
        );
        let asignado = |hora: &str| {
            resultado
                .iter()
                .find(|(h, _)| *h == bogota(JUEVES, hora))
                .map(|(_, b)| *b)
        };
        assert_eq!(asignado("09:00"), Some(IdBarbero(2)), "el primero está ocupado");
        assert_eq!(asignado("10:00"), Some(IdBarbero(1)), "el primero en el orden");
        assert_eq!(resultado.len(), 18, "sin horas repetidas");
    }

    #[test]
    fn clasifica_los_dias_del_calendario() {
        assert_eq!(EstadoDia::por_cantidad(0), EstadoDia::SinCupos);
        assert_eq!(EstadoDia::por_cantidad(3), EstadoDia::UltimosCupos);
        assert_eq!(EstadoDia::por_cantidad(4), EstadoDia::Disponible);
    }

    #[test]
    fn no_se_desborda_a_medianoche() {
        let nocturno = Agendable {
            franjas: vec![Franja {
                dia_semana: 4,
                inicio: h("22:00"),
                fin: NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
            }],
            ..santiago()
        };
        let c = Consulta {
            reglas: Reglas {
                intervalo: TimeDelta::minutes(15),
                ..reglas()
            },
            ..consulta(ahora(), 90, Modo::Publico)
        };
        assert_eq!(
            texto(&horas_del_barbero(&nocturno, d(JUEVES), TipoDia::Normal, &c)),
            ["22:00", "22:15"]
        );
    }
}
