//! Reglas de los clientes, sin dependencias de infraestructura.

use chrono::{Datelike, NaiveDate};
use nucleo::{Celular, Correo, ErrorDeCampo, Validacion};

/// Datos de contacto válidos (RN-07: nombre, celular, correo y fecha de nacimiento opcionales).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatosCliente {
    pub(crate) nombre: String,
    pub(crate) celular: Celular,
    pub(crate) correo: Option<Correo>,
    pub(crate) fecha_nacimiento: Option<NaiveDate>,
}

impl DatosCliente {
    pub(crate) fn nuevos(
        nombre: &str,
        celular: &str,
        correo: Option<&str>,
        fecha_nacimiento: Option<NaiveDate>,
        hoy: NaiveDate,
    ) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let nombre = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
        if !(2..=120).contains(&nombre.chars().count()) {
            v.error("nombre", "Escribe tu nombre (entre 2 y 120 caracteres).");
        }
        let celular = Celular::nuevo(celular)
            .map_err(|e| v.error("celular", e.to_string()))
            .ok();
        let correo = match correo.map(str::trim).filter(|c| !c.is_empty()) {
            None => None,
            Some(texto) => Correo::nuevo(texto)
                .map_err(|e| v.error("correo", e.to_string()))
                .ok(),
        };
        if fecha_nacimiento.is_some_and(|f| f > hoy || hoy.year() - f.year() > 110) {
            v.error("fecha_nacimiento", "Escribe una fecha de nacimiento válida.");
        }
        let errores = v.terminar(|| ());
        match (errores, celular) {
            (Ok(()), Some(celular)) => Ok(Self {
                nombre,
                celular,
                correo,
                fecha_nacimiento,
            }),
            (Err(e), _) => Err(e),
            (Ok(()), None) => Err(vec![ErrorDeCampo::nuevo("celular", "Escribe un celular válido.")]),
        }
    }
}

/// Umbrales de los segmentos (DA-07, RN-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UmbralesSegmentos {
    pub(crate) max_visitas_nuevo: i32,
    pub(crate) max_visitas_recurrente: i32,
    pub(crate) dias_inactividad: i64,
}

/// Nuevo (1 o 2 visitas), Recurrente (3 a 5), Frecuente (6 o más) y Sin volver (última visita
/// hace más de 60 días, prevalece). Sin visitas todavía: `sin_visitas`.
pub(crate) fn segmento(
    visitas: i32,
    ultima_visita: Option<NaiveDate>,
    hoy: NaiveDate,
    u: UmbralesSegmentos,
) -> &'static str {
    if visitas == 0 {
        "sin_visitas"
    } else if ultima_visita.is_some_and(|f| (hoy - f).num_days() > u.dias_inactividad) {
        "inactivo"
    } else if visitas <= u.max_visitas_nuevo {
        "nuevo"
    } else if visitas <= u.max_visitas_recurrente {
        "recurrente"
    } else {
        "frecuente"
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn d(t: &str) -> NaiveDate {
        t.parse().unwrap()
    }

    #[test]
    fn valida_y_normaliza_los_datos() {
        let datos =
            DatosCliente::nuevos(" Andrés   Mora ", "310 555 9917", Some(""), None, d("2026-11-20")).unwrap();
        assert_eq!(
            (datos.nombre.as_str(), datos.celular.digitos(), datos.correo),
            ("Andrés Mora", "3105559917", None)
        );
        let campos: Vec<String> =
            DatosCliente::nuevos("A", "210", Some("no"), Some(d("2030-01-01")), d("2026-11-20"))
                .unwrap_err()
                .into_iter()
                .map(|e| e.campo)
                .collect();
        assert_eq!(campos, ["nombre", "celular", "correo", "fecha_nacimiento"]);
    }

    #[test]
    fn calcula_el_segmento() {
        let u = UmbralesSegmentos {
            max_visitas_nuevo: 2,
            max_visitas_recurrente: 5,
            dias_inactividad: 60,
        };
        let hoy = d("2026-11-20");
        let reciente = Some(d("2026-11-01"));
        assert_eq!(segmento(0, None, hoy, u), "sin_visitas");
        assert_eq!(segmento(2, reciente, hoy, u), "nuevo");
        assert_eq!(segmento(3, reciente, hoy, u), "recurrente");
        assert_eq!(segmento(6, reciente, hoy, u), "frecuente");
        assert_eq!(
            segmento(9, Some(d("2026-09-01")), hoy, u),
            "inactivo",
            "sin volver prevalece"
        );
    }
}
