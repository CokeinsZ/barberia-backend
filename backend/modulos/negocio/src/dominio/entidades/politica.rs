//! Versiones de las políticas legales (RNF-LEG-01): la autorización de un cliente guarda la
//! versión vigente que aceptó.

use chrono::{DateTime, Utc};

use crate::dominio::errores::{ErrorDominio, Validacion};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TipoPolitica {
    TratamientoDatos,
    Cancelacion,
    Terminos,
}

impl TipoPolitica {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            TipoPolitica::TratamientoDatos => "tratamiento_datos",
            TipoPolitica::Cancelacion => "cancelacion",
            TipoPolitica::Terminos => "terminos",
        }
    }

    pub(crate) fn desde_codigo(codigo: &str) -> Option<Self> {
        match codigo {
            "tratamiento_datos" => Some(TipoPolitica::TratamientoDatos),
            "cancelacion" => Some(TipoPolitica::Cancelacion),
            "terminos" => Some(TipoPolitica::Terminos),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevaPolitica {
    pub(crate) tipo: TipoPolitica,
    pub(crate) version: String,
    pub(crate) url: String,
    pub(crate) vigente_desde: DateTime<Utc>,
}

impl NuevaPolitica {
    pub(crate) fn nueva(
        tipo: &str,
        version: &str,
        url: &str,
        vigente_desde: DateTime<Utc>,
    ) -> Result<Self, ErrorDominio> {
        let mut v = Validacion::default();
        let tipo = TipoPolitica::desde_codigo(tipo);
        if tipo.is_none() {
            v.error("tipo", "Elige tratamiento_datos, cancelacion o terminos.");
        }
        let version = version.trim().to_owned();
        if !(1..=20).contains(&version.len())
            || !version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            v.error(
                "version",
                "Escribe la versión con letras, números, punto o guion (por ejemplo 2026.1).",
            );
        }
        let url = url.trim().to_owned();
        let url_valida = url.len() <= 300
            && !url.chars().any(char::is_whitespace)
            && (url.starts_with("https://") || (url.starts_with('/') && !url.starts_with("//")));
        if !url_valida {
            v.error(
                "url",
                "Escribe una dirección https:// o una ruta del sitio que empiece por /.",
            );
        }
        v.terminar(|| Self {
            tipo: tipo.unwrap_or(TipoPolitica::Terminos),
            version,
            url,
            vigente_desde,
        })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn valida_tipo_version_y_url() {
        let ahora = Utc::now();
        assert!(NuevaPolitica::nueva("tratamiento_datos", "2026.1", "/legal/datos", ahora).is_ok());
        assert!(NuevaPolitica::nueva("terminos", "v2", "https://diamante.co/terminos", ahora).is_ok());
        let ErrorDominio::DatosInvalidos(errores) =
            NuevaPolitica::nueva("otra", "con espacios", "javascript:alert(1)", ahora).unwrap_err();
        let campos: Vec<&str> = errores.iter().map(|e| e.campo.as_str()).collect();
        assert_eq!(campos, ["tipo", "version", "url"]);
        assert!(NuevaPolitica::nueva("terminos", "1", "//otro-sitio.com/x", ahora).is_err());
    }
}
