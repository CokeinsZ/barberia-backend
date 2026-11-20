use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errores::ErrorValor;

/// Celular colombiano: 10 dígitos que empiezan por 3, guardado sin indicativo (RNF-LOC-03).
///
/// Acepta formatos habituales al escribirlo: `310 555 9917`, `310-555-9917`, `+57 310 555 9917`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Celular(String);

impl Celular {
    pub fn nuevo(texto: &str) -> Result<Self, ErrorValor> {
        let permitido = |c: char| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+' | '(' | ')' | '.');
        if !texto.chars().all(permitido) {
            return Err(ErrorValor::CelularInvalido);
        }
        let mut digitos: String = texto.chars().filter(char::is_ascii_digit).collect();
        if digitos.len() == 12 && digitos.starts_with("57") {
            digitos.drain(..2);
        }
        if digitos.len() != 10 || !digitos.starts_with('3') {
            return Err(ErrorValor::CelularInvalido);
        }
        Ok(Self(digitos))
    }

    /// Los 10 dígitos, tal como se guardan.
    pub fn digitos(&self) -> &str {
        &self.0
    }

    /// `310 555 9917`
    pub fn formateado(&self) -> String {
        format!("{} {} {}", &self.0[..3], &self.0[3..6], &self.0[6..])
    }

    /// `310 *** 9917`, para mostrarlo sin exponer el número completo (RNF-LEG-05).
    pub fn enmascarado(&self) -> String {
        format!("{} *** {}", &self.0[..3], &self.0[6..])
    }
}

impl TryFrom<String> for Celular {
    type Error = ErrorValor;

    fn try_from(texto: String) -> Result<Self, Self::Error> {
        Celular::nuevo(&texto)
    }
}

impl From<Celular> for String {
    fn from(c: Celular) -> Self {
        c.0
    }
}

impl fmt::Display for Celular {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formateado())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn acepta_formatos_habituales() {
        for texto in [
            "3105559917",
            "310 555 9917",
            "310-555-9917",
            "+57 310 555 9917",
            "(310) 555.9917",
        ] {
            assert_eq!(Celular::nuevo(texto).unwrap().digitos(), "3105559917", "{texto}");
        }
    }

    #[test]
    fn rechaza_lo_que_no_es_celular_colombiano() {
        for texto in [
            "2105559917",
            "310555991",
            "31055599170",
            "310 555 99a7",
            "",
            "+1 310 555 9917",
        ] {
            assert_eq!(Celular::nuevo(texto), Err(ErrorValor::CelularInvalido), "{texto}");
        }
    }

    #[test]
    fn formatea_y_enmascara() {
        let c = Celular::nuevo("3105559917").unwrap();
        assert_eq!(c.formateado(), "310 555 9917");
        assert_eq!(c.enmascarado(), "310 *** 9917");
    }
}
