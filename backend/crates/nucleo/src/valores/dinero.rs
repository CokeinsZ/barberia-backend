use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errores::ErrorValor;

/// Valor en pesos colombianos: entero y nunca negativo (RNF-INT-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Dinero(i64);

impl Dinero {
    pub const CERO: Dinero = Dinero(0);

    /// Crea un valor en pesos. Falla si es negativo.
    pub fn pesos(valor: i64) -> Result<Self, ErrorValor> {
        if valor < 0 {
            return Err(ErrorValor::DineroNegativo(valor));
        }
        Ok(Self(valor))
    }

    pub fn valor(self) -> i64 {
        self.0
    }

    pub fn sumar(self, otro: Dinero) -> Result<Dinero, ErrorValor> {
        self.0
            .checked_add(otro.0)
            .map(Dinero)
            .ok_or(ErrorValor::Desbordamiento)
    }

    /// Resta sin permitir resultados negativos.
    pub fn restar(self, otro: Dinero) -> Result<Dinero, ErrorValor> {
        Dinero::pesos(self.0 - otro.0)
    }
}

impl TryFrom<i64> for Dinero {
    type Error = ErrorValor;

    fn try_from(valor: i64) -> Result<Self, Self::Error> {
        Dinero::pesos(valor)
    }
}

impl From<Dinero> for i64 {
    fn from(d: Dinero) -> Self {
        d.0
    }
}

/// Formato colombiano: `$35.000`.
impl fmt::Display for Dinero {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digitos = self.0.to_string();
        let mut con_puntos = String::with_capacity(digitos.len() + digitos.len() / 3);
        for (i, c) in digitos.chars().enumerate() {
            if i > 0 && (digitos.len() - i).is_multiple_of(3) {
                con_puntos.push('.');
            }
            con_puntos.push(c);
        }
        write!(f, "${con_puntos}")
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn rechaza_valores_negativos() {
        assert_eq!(Dinero::pesos(-1), Err(ErrorValor::DineroNegativo(-1)));
    }

    #[test]
    fn formatea_con_separador_de_miles() {
        assert_eq!(Dinero::pesos(0).unwrap().to_string(), "$0");
        assert_eq!(Dinero::pesos(500).unwrap().to_string(), "$500");
        assert_eq!(Dinero::pesos(35_000).unwrap().to_string(), "$35.000");
        assert_eq!(Dinero::pesos(1_284_500).unwrap().to_string(), "$1.284.500");
    }

    #[test]
    fn restar_no_permite_negativos() {
        let a = Dinero::pesos(5_000).unwrap();
        let b = Dinero::pesos(12_000).unwrap();
        assert!(a.restar(b).is_err());
        assert_eq!(b.restar(a).unwrap().valor(), 7_000);
    }

    #[test]
    fn deserializa_validando() {
        assert!(serde_json::from_str::<Dinero>("-3").is_err());
        assert_eq!(serde_json::from_str::<Dinero>("35000").unwrap().valor(), 35_000);
    }
}
