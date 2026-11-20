use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errores::ErrorValor;

/// Correo electrónico con una validación de forma razonable (no verifica que exista).
/// La comparación sin distinguir mayúsculas la hace la base de datos (`citext`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Correo(String);

impl Correo {
    pub fn nuevo(texto: &str) -> Result<Self, ErrorValor> {
        let correo = texto.trim();
        let valido = correo.len() <= 254
            && !correo.chars().any(char::is_whitespace)
            && correo.matches('@').count() == 1
            && correo.split_once('@').is_some_and(|(usuario, dominio)| {
                !usuario.is_empty()
                    && dominio.rsplit_once('.').is_some_and(|(nombre, tld)| {
                        !nombre.is_empty() && !nombre.starts_with('.') && tld.len() >= 2
                    })
            });
        if !valido {
            return Err(ErrorValor::CorreoInvalido);
        }
        Ok(Self(correo.to_owned()))
    }

    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Correo {
    type Error = ErrorValor;

    fn try_from(texto: String) -> Result<Self, Self::Error> {
        Correo::nuevo(&texto)
    }
}

impl From<Correo> for String {
    fn from(c: Correo) -> Self {
        c.0
    }
}

impl fmt::Display for Correo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn acepta_correos_habituales() {
        for texto in [
            "nombre@correo.com",
            "  laura.martinez@diamante.co ",
            "a+b@sub.dominio.com.co",
        ] {
            assert!(Correo::nuevo(texto).is_ok(), "{texto}");
        }
        assert_eq!(Correo::nuevo(" x@y.co ").unwrap().como_str(), "x@y.co");
    }

    #[test]
    fn rechaza_correos_mal_formados() {
        for texto in [
            "",
            "sin-arroba.com",
            "a@b",
            "a@b.c",
            "@correo.com",
            "a b@correo.com",
            "a@@correo.com",
            "a@.com",
        ] {
            assert_eq!(Correo::nuevo(texto), Err(ErrorValor::CorreoInvalido), "{texto}");
        }
    }
}
