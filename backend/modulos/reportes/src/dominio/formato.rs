//! Formato para personas en español de Colombia (RNF-LOC-01): pesos con separador de miles,
//! fechas largas y horas de 12 horas.

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};

const MESES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const DIAS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];

pub(crate) fn mes(numero: u32) -> &'static str {
    MESES[usize::try_from(numero.clamp(1, 12) - 1).unwrap_or(0)]
}

fn con_puntos(valor: u64) -> String {
    let digitos = valor.to_string();
    let mut texto = String::with_capacity(digitos.len() + digitos.len() / 3);
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i).is_multiple_of(3) {
            texto.push('.');
        }
        texto.push(c);
    }
    texto
}

/// `1.800` (cantidades con separador de miles).
pub(crate) fn numero(valor: i64) -> String {
    let signo = if valor < 0 { "-" } else { "" };
    format!("{signo}{}", con_puntos(valor.unsigned_abs()))
}

/// `$1.234.500` (y `-$2.000` si es negativo).
pub(crate) fn pesos(valor: i64) -> String {
    let signo = if valor < 0 { "-" } else { "" };
    format!("{signo}${}", con_puntos(valor.unsigned_abs()))
}

/// Un decimal con coma y sin «,0»: `67,8`, `45`, `37,5`.
pub(crate) fn decimal(valor: f64) -> String {
    let texto = format!("{valor:.1}").replace('.', ",");
    texto.strip_suffix(",0").map_or(texto.clone(), str::to_owned)
}

/// «lunes 23 de noviembre de 2026».
pub(crate) fn fecha_larga(fecha: NaiveDate) -> String {
    format!(
        "{} {} de {} de {}",
        DIAS[usize::try_from(fecha.weekday().num_days_from_monday()).unwrap_or(0)],
        fecha.day(),
        mes(fecha.month()),
        fecha.year()
    )
}

/// «23/11/2026».
pub(crate) fn fecha_corta(fecha: NaiveDate) -> String {
    fecha.format("%d/%m/%Y").to_string()
}

/// «9:00 a. m.», «3:30 p. m.».
pub(crate) fn hora(hora: NaiveTime) -> String {
    let (pm, h) = hora.hour12();
    format!("{h}:{:02} {}", hora.minute(), if pm { "p. m." } else { "a. m." })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn numeros_y_decimales_a_la_colombiana() {
        assert_eq!(
            (numero(1_800), numero(999), numero(-12_345)),
            ("1.800".into(), "999".into(), "-12.345".into())
        );
        assert_eq!(
            (decimal(67.8), decimal(45.0), decimal(37.5), decimal(-15.7)),
            ("67,8".into(), "45".into(), "37,5".into(), "-15,7".into())
        );
    }

    #[test]
    fn formatea_pesos_fechas_y_horas() {
        assert_eq!(pesos(0), "$0");
        assert_eq!(pesos(950), "$950");
        assert_eq!(pesos(35_000), "$35.000");
        assert_eq!(pesos(1_234_500), "$1.234.500");
        assert_eq!(pesos(-2_000), "-$2.000");
        let fecha = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        assert_eq!(fecha_larga(fecha), "lunes 23 de noviembre de 2026");
        assert_eq!(fecha_corta(fecha), "23/11/2026");
        assert_eq!(hora(NaiveTime::from_hms_opt(9, 0, 0).unwrap()), "9:00 a. m.");
        assert_eq!(hora(NaiveTime::from_hms_opt(15, 30, 0).unwrap()), "3:30 p. m.");
        assert_eq!(hora(NaiveTime::from_hms_opt(12, 5, 0).unwrap()), "12:05 p. m.");
    }
}
