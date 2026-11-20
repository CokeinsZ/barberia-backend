use chrono::{NaiveDate, NaiveTime};

use crate::dominio::errores::{ErrorDominio, Validacion};

/// Qué pasa ese día (RN-03, RF-CFG-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoDiaEspecial {
    /// Cada barbero trabaja con su horario de domingo.
    Festivo,
    /// No se ofrecen horas.
    Cerrado,
    /// Se ofrecen horas solo hasta la hora de cierre.
    CierreTemprano { hora_cierre: NaiveTime },
}

impl TipoDiaEspecial {
    /// Valor del enum `negocio.tipo_dia_especial` en PostgreSQL.
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            TipoDiaEspecial::Festivo => "festivo",
            TipoDiaEspecial::Cerrado => "cerrado",
            TipoDiaEspecial::CierreTemprano { .. } => "cierre_temprano",
        }
    }

    pub(crate) fn hora_cierre(self) -> Option<NaiveTime> {
        match self {
            TipoDiaEspecial::CierreTemprano { hora_cierre } => Some(hora_cierre),
            _ => None,
        }
    }

    /// Construye el tipo a partir de su código y la hora de cierre, validando su coherencia.
    pub(crate) fn desde_codigo(codigo: &str, hora_cierre: Option<NaiveTime>) -> Result<Self, ErrorDominio> {
        let mut v = Validacion::default();
        let tipo = match (codigo, hora_cierre) {
            ("festivo", None) => Some(TipoDiaEspecial::Festivo),
            ("cerrado", None) => Some(TipoDiaEspecial::Cerrado),
            ("cierre_temprano", Some(hora)) if hora > NaiveTime::MIN => {
                Some(TipoDiaEspecial::CierreTemprano { hora_cierre: hora })
            }
            ("cierre_temprano", _) => {
                v.error("hora_cierre", "Indica la hora de cierre.");
                None
            }
            ("festivo" | "cerrado", Some(_)) => {
                v.error("hora_cierre", "Solo el cierre temprano lleva hora de cierre.");
                None
            }
            _ => {
                v.error("tipo", "Elige festivo, cerrado o cierre temprano.");
                None
            }
        };
        v.terminar(|| tipo.unwrap_or(TipoDiaEspecial::Cerrado))
    }
}

/// Origen del registro: precargado (festivos de Colombia) o creado por el administrador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrigenDiaEspecial {
    Automatico,
    Manual,
}

impl OrigenDiaEspecial {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            OrigenDiaEspecial::Automatico => "automatico",
            OrigenDiaEspecial::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiaEspecial {
    fecha: NaiveDate,
    nombre: String,
    tipo: TipoDiaEspecial,
    origen: OrigenDiaEspecial,
}

impl DiaEspecial {
    /// Registro manual del administrador: no admite fechas pasadas.
    pub(crate) fn registrar(
        fecha: NaiveDate,
        nombre: &str,
        tipo: TipoDiaEspecial,
        hoy: NaiveDate,
    ) -> Result<Self, ErrorDominio> {
        let mut v = Validacion::default();
        if fecha < hoy {
            v.error(
                "fecha",
                "No se pueden registrar días especiales en fechas pasadas.",
            );
        }
        let nombre = nombre.trim().to_owned();
        if !(2..=80).contains(&nombre.chars().count()) {
            v.error(
                "nombre",
                "Escribe un nombre entre 2 y 80 caracteres, por ejemplo «Nochebuena».",
            );
        }
        v.terminar(|| Self {
            fecha,
            nombre,
            tipo,
            origen: OrigenDiaEspecial::Manual,
        })
    }

    /// Reconstruye un día ya guardado (los datos se validaron al registrarlo).
    pub(crate) fn reconstruir(
        fecha: NaiveDate,
        nombre: String,
        tipo: TipoDiaEspecial,
        origen: OrigenDiaEspecial,
    ) -> Self {
        Self {
            fecha,
            nombre,
            tipo,
            origen,
        }
    }

    pub(crate) fn fecha(&self) -> NaiveDate {
        self.fecha
    }
    pub(crate) fn nombre(&self) -> &str {
        &self.nombre
    }
    pub(crate) fn tipo(&self) -> TipoDiaEspecial {
        self.tipo
    }
    pub(crate) fn origen(&self) -> OrigenDiaEspecial {
        self.origen
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn fecha(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    fn campos(error: ErrorDominio) -> Vec<String> {
        let ErrorDominio::DatosInvalidos(errores) = error;
        errores.into_iter().map(|e| e.campo).collect()
    }

    #[test]
    fn el_cierre_temprano_exige_hora() {
        assert_eq!(
            campos(TipoDiaEspecial::desde_codigo("cierre_temprano", None).unwrap_err()),
            ["hora_cierre"]
        );
        let tipo = TipoDiaEspecial::desde_codigo(
            "cierre_temprano",
            Some(NaiveTime::from_hms_opt(14, 0, 0).unwrap()),
        );
        assert_eq!(tipo.unwrap().hora_cierre(), NaiveTime::from_hms_opt(14, 0, 0));
    }

    #[test]
    fn solo_el_cierre_temprano_lleva_hora() {
        let hora = NaiveTime::from_hms_opt(14, 0, 0);
        assert_eq!(
            campos(TipoDiaEspecial::desde_codigo("cerrado", hora).unwrap_err()),
            ["hora_cierre"]
        );
        assert_eq!(
            campos(TipoDiaEspecial::desde_codigo("vacaciones", None).unwrap_err()),
            ["tipo"]
        );
    }

    #[test]
    fn no_admite_fechas_pasadas_ni_nombres_vacios() {
        let error = DiaEspecial::registrar(
            fecha("2026-11-17"),
            " ",
            TipoDiaEspecial::Cerrado,
            fecha("2026-11-18"),
        );
        assert_eq!(campos(error.unwrap_err()), ["fecha", "nombre"]);
        let hoy = DiaEspecial::registrar(
            fecha("2026-11-18"),
            "Inventario",
            TipoDiaEspecial::Cerrado,
            fecha("2026-11-18"),
        );
        assert_eq!(hoy.unwrap().origen(), OrigenDiaEspecial::Manual);
    }
}
