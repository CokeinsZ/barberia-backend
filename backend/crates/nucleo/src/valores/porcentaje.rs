use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errores::ErrorValor;
use crate::valores::Dinero;

/// Porcentaje con dos decimales, de 0,00 a 100,00. Se guarda en centésimas
/// (`5000` = 50 %) para operar sin decimales de punto flotante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Porcentaje(u16);

impl Porcentaje {
    pub const CERO: Porcentaje = Porcentaje(0);
    const MAXIMO: u16 = 10_000;

    pub fn desde_centesimas(centesimas: u16) -> Result<Self, ErrorValor> {
        if centesimas > Self::MAXIMO {
            return Err(ErrorValor::PorcentajeFueraDeRango);
        }
        Ok(Self(centesimas))
    }

    pub fn entero(porcentaje: u8) -> Result<Self, ErrorValor> {
        Self::desde_centesimas(u16::from(porcentaje) * 100)
    }

    pub fn centesimas(self) -> u16 {
        self.0
    }

    /// Aplica el porcentaje a un valor y redondea al peso más cercano
    /// (las mitades hacia arriba, igual que `round()` de PostgreSQL con valores positivos).
    pub fn aplicar(self, base: Dinero) -> Dinero {
        let producto = i128::from(base.valor()) * i128::from(self.0);
        let redondeado = (producto + 5_000) / 10_000;
        // El resultado nunca supera la base, que ya cabe en i64 y no es negativa.
        Dinero::pesos(i64::try_from(redondeado).unwrap_or(i64::MAX)).unwrap_or(Dinero::CERO)
    }
}

impl fmt::Display for Porcentaje {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (enteros, decimales) = (self.0 / 100, self.0 % 100);
        if decimales == 0 {
            write!(f, "{enteros} %")
        } else if decimales.is_multiple_of(10) {
            write!(f, "{enteros},{} %", decimales / 10)
        } else {
            write!(f, "{enteros},{decimales:02} %")
        }
    }
}

/// En JSON viaja como número: `50` o `12.5`.
impl Serialize for Porcentaje {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(f64::from(self.0) / 100.0)
    }
}

impl<'de> Deserialize<'de> for Porcentaje {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let valor = f64::deserialize(d)?;
        if !(0.0..=100.0).contains(&valor) {
            return Err(serde::de::Error::custom(ErrorValor::PorcentajeFueraDeRango));
        }
        let centesimas = (valor * 100.0).round();
        // El rango ya se validó: 0..=10000 cabe en u16.
        Porcentaje::desde_centesimas(centesimas as u16).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn aplica_y_redondea_al_peso() {
        let cincuenta = Porcentaje::entero(50).unwrap();
        assert_eq!(cincuenta.aplicar(Dinero::pesos(49_500).unwrap()).valor(), 24_750);
        let diez = Porcentaje::entero(10).unwrap();
        assert_eq!(diez.aplicar(Dinero::pesos(67_000).unwrap()).valor(), 6_700);
        let doce_y_medio = Porcentaje::desde_centesimas(1_250).unwrap();
        assert_eq!(doce_y_medio.aplicar(Dinero::pesos(1_004).unwrap()).valor(), 126); // 125,5 → 126
    }

    #[test]
    fn rechaza_mas_de_cien() {
        assert!(Porcentaje::desde_centesimas(10_001).is_err());
        assert!(serde_json::from_str::<Porcentaje>("100.5").is_err());
    }

    #[test]
    fn formatea_en_espanol() {
        assert_eq!(Porcentaje::entero(50).unwrap().to_string(), "50 %");
        assert_eq!(Porcentaje::desde_centesimas(1_250).unwrap().to_string(), "12,5 %");
        assert_eq!(
            Porcentaje::desde_centesimas(1_275).unwrap().to_string(),
            "12,75 %"
        );
    }

    #[test]
    fn viaja_como_numero_en_json() {
        let p: Porcentaje = serde_json::from_str("12.5").unwrap();
        assert_eq!(p.centesimas(), 1_250);
        assert_eq!(serde_json::to_string(&p).unwrap(), "12.5");
    }
}
