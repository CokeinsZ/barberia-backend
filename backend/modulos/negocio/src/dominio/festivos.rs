//! Festivos de Colombia (RF-CFG-05): fijos, trasladables al lunes siguiente (Ley 51 de 1983,
//! «Ley Emiliani») y los que dependen de la Pascua.

use chrono::{Datelike, Days, NaiveDate};

/// Domingo de Pascua (algoritmo anónimo gregoriano de Meeus/Jones/Butcher).
pub(crate) fn pascua(anio: i32) -> Option<NaiveDate> {
    let a = anio % 19;
    let b = anio / 100;
    let c = anio % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let mes = (h + l - 7 * m + 114) / 31;
    let dia = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(anio, u32::try_from(mes).ok()?, u32::try_from(dia).ok()?)
}

/// La misma fecha si es lunes; si no, el lunes siguiente.
fn al_lunes(fecha: NaiveDate) -> Option<NaiveDate> {
    let dias = (7 - fecha.weekday().num_days_from_monday()) % 7;
    fecha.checked_add_days(Days::new(u64::from(dias)))
}

fn desplazar(fecha: NaiveDate, dias: i64) -> Option<NaiveDate> {
    if dias >= 0 {
        fecha.checked_add_days(Days::new(dias.unsigned_abs()))
    } else {
        fecha.checked_sub_days(Days::new(dias.unsigned_abs()))
    }
}

/// Los festivos del año, ordenados por fecha. Si dos caen el mismo día (p. ej. el 30 de junio
/// de 2025), se unen en uno solo.
pub(crate) fn festivos_de_colombia(anio: i32) -> Vec<(NaiveDate, String)> {
    let fecha = |mes, dia| NaiveDate::from_ymd_opt(anio, mes, dia);
    let fijos = [
        (fecha(1, 1), "Año Nuevo"),
        (fecha(5, 1), "Día del Trabajo"),
        (fecha(7, 20), "Día de la Independencia"),
        (fecha(8, 7), "Batalla de Boyacá"),
        (fecha(12, 8), "Inmaculada Concepción"),
        (fecha(12, 25), "Navidad"),
    ];
    let trasladables = [
        (fecha(1, 6), "Día de los Reyes Magos"),
        (fecha(3, 19), "Día de San José"),
        (fecha(6, 29), "San Pedro y San Pablo"),
        (fecha(8, 15), "Asunción de la Virgen"),
        (fecha(10, 12), "Día de la Raza"),
        (fecha(11, 1), "Todos los Santos"),
        (fecha(11, 11), "Independencia de Cartagena"),
    ];
    let mut festivos: Vec<(NaiveDate, &'static str)> = fijos
        .into_iter()
        .filter_map(|(f, n)| Some((f?, n)))
        .chain(
            trasladables
                .into_iter()
                .filter_map(|(f, n)| Some((al_lunes(f?)?, n))),
        )
        .collect();
    if let Some(domingo) = pascua(anio) {
        let relativos = [
            (desplazar(domingo, -3), "Jueves Santo"),
            (desplazar(domingo, -2), "Viernes Santo"),
            // Ascensión, Corpus Christi y Sagrado Corazón ya caen en lunes con estos desplazamientos.
            (desplazar(domingo, 43), "Ascensión del Señor"),
            (desplazar(domingo, 64), "Corpus Christi"),
            (desplazar(domingo, 71), "Sagrado Corazón"),
        ];
        festivos.extend(relativos.into_iter().filter_map(|(f, n)| Some((f?, n))));
    }
    festivos.sort_by_key(|(f, _)| *f);
    let mut unidos: Vec<(NaiveDate, String)> = Vec::with_capacity(festivos.len());
    for (fecha, nombre) in festivos {
        match unidos.last_mut() {
            Some((ultima, nombres)) if *ultima == fecha => {
                nombres.push_str(" · ");
                nombres.push_str(nombre);
            }
            _ => unidos.push((fecha, nombre.to_owned())),
        }
    }
    unidos
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn d(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    #[test]
    fn calcula_la_pascua() {
        assert_eq!(pascua(2024), Some(d("2024-03-31")));
        assert_eq!(pascua(2025), Some(d("2025-04-20")));
        assert_eq!(pascua(2026), Some(d("2026-04-05")));
        assert_eq!(pascua(2027), Some(d("2027-03-28")));
    }

    #[test]
    fn festivos_oficiales_de_2026() {
        let fechas: Vec<String> = festivos_de_colombia(2026)
            .iter()
            .map(|(f, _)| f.to_string())
            .collect();
        assert_eq!(
            fechas,
            [
                "2026-01-01",
                "2026-01-12",
                "2026-03-23",
                "2026-04-02",
                "2026-04-03",
                "2026-05-01",
                "2026-05-18",
                "2026-06-08",
                "2026-06-15",
                "2026-06-29",
                "2026-07-20",
                "2026-08-07",
                "2026-08-17",
                "2026-10-12",
                "2026-11-02",
                "2026-11-16",
                "2026-12-08",
                "2026-12-25",
            ]
        );
    }

    #[test]
    fn festivos_oficiales_de_2025_con_dos_el_mismo_dia() {
        let festivos = festivos_de_colombia(2025);
        assert_eq!(festivos.len(), 17, "el 30 de junio une dos festivos");
        let buscar = |nombre: &str| {
            festivos
                .iter()
                .find(|(_, n)| n.contains(nombre))
                .unwrap()
                .0
                .to_string()
        };
        assert_eq!(buscar("Reyes Magos"), "2025-01-06");
        assert_eq!(buscar("Ascensión"), "2025-06-02");
        assert_eq!(buscar("Corpus Christi"), "2025-06-23");
        assert_eq!(buscar("Sagrado Corazón"), "2025-06-30");
        assert_eq!(buscar("Todos los Santos"), "2025-11-03");
        assert!(
            festivos
                .iter()
                .any(|(_, n)| n == "San Pedro y San Pablo · Sagrado Corazón")
        );
    }
}
