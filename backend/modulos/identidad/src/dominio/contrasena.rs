use super::ErrorDominio;

/// Contraseña elegida por la persona. Longitud de 12 a 128 caracteres (OWASP ASVS 2.1): se
/// prefiere una frase larga a reglas de composición.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ContrasenaNueva(String);

impl ContrasenaNueva {
    pub(crate) const MINIMO: usize = 12;
    pub(crate) const MAXIMO: usize = 128;

    pub(crate) fn nueva(texto: String) -> Result<Self, ErrorDominio> {
        let caracteres = texto.chars().count();
        if !(Self::MINIMO..=Self::MAXIMO).contains(&caracteres) || texto.trim().is_empty() {
            return Err(ErrorDominio::LongitudContrasena);
        }
        Ok(Self(texto))
    }

    pub(crate) fn exponer(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ContrasenaNueva {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContrasenaNueva(***)")
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn exige_entre_12_y_128_caracteres() {
        assert!(ContrasenaNueva::nueva("corta".into()).is_err());
        assert!(ContrasenaNueva::nueva(" ".repeat(20)).is_err());
        assert!(ContrasenaNueva::nueva("ñ".repeat(129)).is_err());
        // Se cuentan caracteres, no bytes.
        assert!(ContrasenaNueva::nueva("ñandú-tijera".into()).is_ok());
        assert!(ContrasenaNueva::nueva("x".repeat(128)).is_ok());
    }

    #[test]
    fn no_se_imprime() {
        let clave = ContrasenaNueva::nueva("una frase bastante larga".into()).unwrap();
        assert_eq!(format!("{clave:?}"), "ContrasenaNueva(***)");
    }
}
