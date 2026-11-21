//! Horario semanal del barbero (RF-BAR-07): hasta tres franjas por día, sin cruces.

use chrono::NaiveTime;
use nucleo::{ErrorDeCampo, Validacion};

pub(crate) const DIAS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];
const MAX_FRANJAS_POR_DIA: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Franja {
    /// 1 = lunes … 7 = domingo (ISO 8601).
    pub(crate) dia_semana: u8,
    pub(crate) inicio: NaiveTime,
    pub(crate) fin: NaiveTime,
}

impl Franja {
    pub(crate) fn minutos(self) -> i64 {
        (self.fin - self.inicio).num_minutes()
    }
}

/// Horario válido, ordenado por día y hora.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Horario(Vec<Franja>);

impl Horario {
    pub(crate) fn nuevo(mut franjas: Vec<Franja>) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        franjas.sort();
        for franja in &franjas {
            if !(1..=7).contains(&franja.dia_semana) {
                v.error("horario", "El día de la semana va de 1 (lunes) a 7 (domingo).");
                return v.terminar(Self::default);
            }
            if franja.fin <= franja.inicio {
                v.error(
                    "horario",
                    format!(
                        "El {}: la hora final debe ser posterior a la inicial.",
                        DIAS[usize::from(franja.dia_semana - 1)]
                    ),
                );
            }
        }
        for dia in 1..=7_u8 {
            let del_dia: Vec<&Franja> = franjas.iter().filter(|f| f.dia_semana == dia).collect();
            let nombre = DIAS[usize::from(dia - 1)];
            if del_dia.len() > MAX_FRANJAS_POR_DIA {
                v.error(
                    "horario",
                    format!("El {nombre} tiene más de {MAX_FRANJAS_POR_DIA} franjas."),
                );
            }
            if del_dia.windows(2).any(|par| par[1].inicio < par[0].fin) {
                v.error("horario", format!("El {nombre} tiene franjas que se cruzan."));
            }
        }
        v.terminar(|| Self(franjas))
    }

    /// Reconstruye un horario guardado (ya se validó al guardarlo).
    pub(crate) fn reconstruir(mut franjas: Vec<Franja>) -> Self {
        franjas.sort();
        Self(franjas)
    }

    pub(crate) fn franjas(&self) -> &[Franja] {
        &self.0
    }

    pub(crate) fn minutos_semana(&self) -> i64 {
        self.0.iter().map(|f| f.minutos()).sum()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn franja(dia: u8, inicio: &str, fin: &str) -> Franja {
        Franja {
            dia_semana: dia,
            inicio: inicio.parse().unwrap(),
            fin: fin.parse().unwrap(),
        }
    }

    #[test]
    fn suma_las_horas_de_la_semana() {
        let horario = Horario::nuevo(vec![
            franja(2, "14:00:00", "19:00:00"),
            franja(1, "09:00:00", "13:00:00"),
            franja(1, "14:00:00", "19:00:00"),
        ])
        .unwrap();
        assert_eq!(
            horario.franjas()[0],
            franja(1, "09:00:00", "13:00:00"),
            "queda ordenado"
        );
        assert_eq!(horario.minutos_semana(), 14 * 60);
        assert_eq!(Horario::nuevo(Vec::new()).unwrap().minutos_semana(), 0);
    }

    #[test]
    fn rechaza_franjas_invertidas_cruzadas_o_demasiadas() {
        let errores = |franjas| {
            Horario::nuevo(franjas)
                .unwrap_err()
                .into_iter()
                .map(|e| e.mensaje)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            errores(vec![franja(1, "13:00:00", "09:00:00")]),
            ["El lunes: la hora final debe ser posterior a la inicial."]
        );
        assert_eq!(
            errores(vec![
                franja(6, "09:00:00", "13:00:00"),
                franja(6, "12:00:00", "18:00:00")
            ]),
            ["El sábado tiene franjas que se cruzan."]
        );
        let cuatro = vec![
            franja(3, "08:00:00", "09:00:00"),
            franja(3, "10:00:00", "11:00:00"),
            franja(3, "12:00:00", "13:00:00"),
            franja(3, "14:00:00", "15:00:00"),
        ];
        assert_eq!(errores(cuatro), ["El miércoles tiene más de 3 franjas."]);
        assert!(Horario::nuevo(vec![franja(8, "08:00:00", "09:00:00")]).is_err());
        // Franjas contiguas sí se permiten.
        assert!(
            Horario::nuevo(vec![
                franja(5, "08:00:00", "12:00:00"),
                franja(5, "12:00:00", "16:00:00")
            ])
            .is_ok()
        );
    }
}
