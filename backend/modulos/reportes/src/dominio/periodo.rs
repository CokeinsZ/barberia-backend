//! Periodo de un reporte (RF-REP-01, RN-22): un mes (el mes en curso cuenta hasta hoy) o un
//! rango de fechas que no termina después de hoy.

use chrono::{Datelike, Days, Months, NaiveDate};
use nucleo::{ErrorDeCampo, Validacion};
use serde::Serialize;

use super::formato::mes;

/// Un año: más que eso no es un reporte, es una exportación.
const MAXIMO_DIAS: i64 = 366;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Periodo {
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
}

impl Periodo {
    /// Con `mes` (`AAAA-MM`) o con `desde` y `hasta`. Sin nada: el mes en curso hasta hoy.
    pub(crate) fn desde_parametros(
        mes: Option<&str>,
        desde: Option<NaiveDate>,
        hasta: Option<NaiveDate>,
        hoy: NaiveDate,
    ) -> Result<Periodo, Vec<ErrorDeCampo>> {
        match (mes, desde, hasta) {
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(vec![ErrorDeCampo::nuevo(
                "mes",
                "Usa el mes o el rango de fechas, no los dos.",
            )]),
            (Some(texto), None, None) => Periodo::mes(texto, hoy),
            (None, None, None) => Periodo::mes(&hoy.format("%Y-%m").to_string(), hoy),
            (None, desde, hasta) => {
                let mut v = Validacion::default();
                if desde.is_none() {
                    v.error("desde", "Indica la fecha inicial.");
                }
                if hasta.is_none() {
                    v.error("hasta", "Indica la fecha final.");
                }
                v.terminar(|| ())?;
                Periodo::rango(desde.unwrap_or(hoy), hasta.unwrap_or(hoy), hoy)
            }
        }
    }

    /// `AAAA-MM`; el mes en curso termina hoy y un mes futuro no se puede pedir.
    pub(crate) fn mes(texto: &str, hoy: NaiveDate) -> Result<Periodo, Vec<ErrorDeCampo>> {
        let error = |m: &str| vec![ErrorDeCampo::nuevo("mes", m)];
        let inicio = NaiveDate::parse_from_str(&format!("{texto}-01"), "%Y-%m-%d")
            .map_err(|_| error("Usa el formato AAAA-MM, p. ej. 2026-11."))?;
        if inicio > hoy {
            return Err(error("El mes todavía no ha empezado."));
        }
        let fin_de_mes = inicio
            .checked_add_months(Months::new(1))
            .and_then(|d| d.checked_sub_days(Days::new(1)))
            .unwrap_or(inicio);
        Ok(Periodo {
            desde: inicio,
            hasta: fin_de_mes.min(hoy),
        })
    }

    pub(crate) fn rango(
        desde: NaiveDate,
        hasta: NaiveDate,
        hoy: NaiveDate,
    ) -> Result<Periodo, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        if hasta > hoy {
            v.error("hasta", "Un rango no puede terminar después de hoy.");
        }
        if desde > hasta {
            v.error("desde", "La fecha inicial debe ser anterior a la final.");
        } else if (hasta - desde).num_days() >= MAXIMO_DIAS {
            v.error("hasta", "El rango puede ser de hasta un año.");
        }
        v.terminar(|| Periodo { desde, hasta })
    }

    pub(crate) fn dias(&self) -> i64 {
        (self.hasta - self.desde).num_days() + 1
    }

    /// «1 al 21 de noviembre de 2026», «28 de octubre al 5 de noviembre de 2026»,
    /// «15 de diciembre de 2025 al 10 de enero de 2026» o «23 de noviembre de 2026».
    pub(crate) fn texto(&self) -> String {
        let (d, h) = (self.desde, self.hasta);
        if d == h {
            format!("{} de {} de {}", d.day(), mes(d.month()), d.year())
        } else if d.year() != h.year() {
            format!(
                "{} de {} de {} al {} de {} de {}",
                d.day(),
                mes(d.month()),
                d.year(),
                h.day(),
                mes(h.month()),
                h.year()
            )
        } else if d.month() != h.month() {
            format!(
                "{} de {} al {} de {} de {}",
                d.day(),
                mes(d.month()),
                h.day(),
                mes(h.month()),
                h.year()
            )
        } else {
            format!("{} al {} de {} de {}", d.day(), h.day(), mes(h.month()), h.year())
        }
    }

    /// Semanas de lunes a domingo, recortadas al periodo (para «ventas por semana»).
    pub(crate) fn semanas(&self) -> Vec<Periodo> {
        let mut semanas = Vec::new();
        let mut inicio = self.desde;
        while inicio <= self.hasta {
            let domingo = inicio
                .checked_add_days(Days::new(u64::from(6 - inicio.weekday().num_days_from_monday())))
                .unwrap_or(inicio);
            let fin = domingo.min(self.hasta);
            semanas.push(Periodo {
                desde: inicio,
                hasta: fin,
            });
            match fin.checked_add_days(Days::new(1)) {
                Some(siguiente) => inicio = siguiente,
                None => break,
            }
        }
        semanas
    }

    /// Nombre corto para archivos: `2026-11` si es un mes completo o el mes en curso, si no
    /// `2026-10-28_2026-11-05`.
    pub(crate) fn para_archivo(&self) -> String {
        if self.desde.day() == 1
            && self.desde.year() == self.hasta.year()
            && self.desde.month() == self.hasta.month()
        {
            self.desde.format("%Y-%m").to_string()
        } else {
            format!("{}_{}", self.desde, self.hasta)
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn f(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    const HOY: &str = "2026-11-21";

    #[test]
    fn el_mes_en_curso_termina_hoy_y_los_anteriores_completos() {
        assert_eq!(
            Periodo::mes("2026-11", f(HOY)).unwrap(),
            Periodo {
                desde: f("2026-11-01"),
                hasta: f(HOY)
            }
        );
        assert_eq!(
            Periodo::mes("2026-02", f(HOY)).unwrap(),
            Periodo {
                desde: f("2026-02-01"),
                hasta: f("2026-02-28")
            }
        );
        assert_eq!(
            Periodo::mes("2026-12", f(HOY)).unwrap_err()[0].campo,
            "mes",
            "mes futuro"
        );
        assert!(Periodo::mes("noviembre", f(HOY)).is_err());
        assert_eq!(
            Periodo::desde_parametros(None, None, None, f(HOY)).unwrap(),
            Periodo {
                desde: f("2026-11-01"),
                hasta: f(HOY)
            },
            "sin parámetros: el mes en curso"
        );
    }

    #[test]
    fn un_rango_no_termina_despues_de_hoy_ni_pasa_de_un_anio() {
        assert!(Periodo::rango(f("2026-10-28"), f("2026-11-05"), f(HOY)).is_ok());
        assert_eq!(
            Periodo::rango(f("2026-11-01"), f("2026-11-30"), f(HOY)).unwrap_err()[0].campo,
            "hasta",
            "RN-22"
        );
        assert_eq!(
            Periodo::rango(f("2026-11-10"), f("2026-11-01"), f(HOY)).unwrap_err()[0].campo,
            "desde"
        );
        assert!(Periodo::rango(f("2025-11-01"), f("2026-11-05"), f(HOY)).is_err());
        let errores = Periodo::desde_parametros(None, Some(f("2026-11-01")), None, f(HOY)).unwrap_err();
        assert_eq!(errores[0].campo, "hasta");
        assert!(Periodo::desde_parametros(Some("2026-11"), Some(f("2026-11-01")), None, f(HOY)).is_err());
    }

    #[test]
    fn textos_y_semanas() {
        let noviembre = Periodo::mes("2026-11", f(HOY)).unwrap();
        assert_eq!(noviembre.texto(), "1 al 21 de noviembre de 2026");
        assert_eq!(noviembre.dias(), 21);
        assert_eq!(noviembre.para_archivo(), "2026-11");
        let cruzado = Periodo::rango(f("2025-12-15"), f("2026-01-10"), f(HOY)).unwrap();
        assert_eq!(cruzado.texto(), "15 de diciembre de 2025 al 10 de enero de 2026");
        assert_eq!(cruzado.para_archivo(), "2025-12-15_2026-01-10");
        let dos_meses = Periodo::rango(f("2026-10-28"), f("2026-11-05"), f(HOY)).unwrap();
        assert_eq!(dos_meses.texto(), "28 de octubre al 5 de noviembre de 2026");
        // Noviembre de 2026 empieza en domingo: la primera «semana» es solo el 1.
        let semanas: Vec<(NaiveDate, NaiveDate)> =
            noviembre.semanas().iter().map(|s| (s.desde, s.hasta)).collect();
        assert_eq!(
            semanas,
            [
                (f("2026-11-01"), f("2026-11-01")),
                (f("2026-11-02"), f("2026-11-08")),
                (f("2026-11-09"), f("2026-11-15")),
                (f("2026-11-16"), f("2026-11-21")),
            ]
        );
    }
}
