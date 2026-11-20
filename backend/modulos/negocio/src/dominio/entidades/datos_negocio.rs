use nucleo::{Celular, Porcentaje};

use crate::dominio::errores::{ErrorDominio, Validacion, texto_opcional};

/// Datos de la barbería (una sola por instalación). Siempre válidos: solo se construyen con
/// [`DatosNegocio::nuevo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatosNegocio {
    nombre_comercial: String,
    nit: Option<String>,
    direccion: Option<String>,
    zona_referencia: Option<String>,
    whatsapp: Option<Celular>,
    instagram: Option<String>,
    porcentaje_comision_defecto: Porcentaje,
}

/// Datos tal como llegan, sin validar.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropuestaDatosNegocio {
    pub(crate) nombre_comercial: String,
    pub(crate) nit: Option<String>,
    pub(crate) direccion: Option<String>,
    pub(crate) zona_referencia: Option<String>,
    pub(crate) whatsapp: Option<String>,
    pub(crate) instagram: Option<String>,
    pub(crate) porcentaje_comision_defecto: f64,
}

impl DatosNegocio {
    pub(crate) fn nuevo(propuesta: PropuestaDatosNegocio) -> Result<Self, ErrorDominio> {
        let mut v = Validacion::default();

        let nombre_comercial = propuesta.nombre_comercial.trim().to_owned();
        if !(2..=120).contains(&nombre_comercial.chars().count()) {
            v.error(
                "nombre_comercial",
                "Escribe el nombre comercial (entre 2 y 120 caracteres).",
            );
        }

        let nit = texto_opcional(propuesta.nit);
        if let Some(nit) = &nit {
            let digitos: String = nit.chars().filter(|c| *c != '.').collect();
            let (base, verificacion) = digitos.split_once('-').unwrap_or((&digitos, "0"));
            let valido = (6..=10).contains(&base.len())
                && base.chars().all(|c| c.is_ascii_digit())
                && verificacion.len() == 1
                && verificacion.chars().all(|c| c.is_ascii_digit());
            if !valido {
                v.error(
                    "nit",
                    "Escribe el NIT con su dígito de verificación, por ejemplo 900.123.456-7.",
                );
            }
        }

        let direccion = texto_opcional(propuesta.direccion);
        if direccion.as_ref().is_some_and(|d| d.chars().count() > 200) {
            v.error("direccion", "La dirección admite máximo 200 caracteres.");
        }
        let zona_referencia = texto_opcional(propuesta.zona_referencia);
        if zona_referencia.as_ref().is_some_and(|z| z.chars().count() > 200) {
            v.error("zona_referencia", "La referencia admite máximo 200 caracteres.");
        }

        let whatsapp = match texto_opcional(propuesta.whatsapp) {
            None => None,
            Some(texto) => match Celular::nuevo(&texto) {
                Ok(celular) => Some(celular),
                Err(error) => {
                    v.error("whatsapp", error.to_string());
                    None
                }
            },
        };

        let instagram = texto_opcional(propuesta.instagram).map(|i| i.trim_start_matches('@').to_owned());
        if instagram.as_ref().is_some_and(|i| {
            i.is_empty()
                || i.len() > 30
                || !i
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        }) {
            v.error(
                "instagram",
                "Escribe el usuario de Instagram sin espacios (letras, números, punto o guion bajo).",
            );
        }

        let porcentaje = propuesta.porcentaje_comision_defecto;
        let porcentaje_comision_defecto = if (0.0..=100.0).contains(&porcentaje) {
            // Ya está en rango: 0..=10000 cabe en u16.
            Porcentaje::desde_centesimas((porcentaje * 100.0).round() as u16).ok()
        } else {
            None
        };
        if porcentaje_comision_defecto.is_none() {
            v.error(
                "porcentaje_comision_defecto",
                "La comisión debe estar entre 0 y 100 %.",
            );
        }

        v.terminar(|| Self {
            nombre_comercial,
            nit,
            direccion,
            zona_referencia,
            whatsapp,
            instagram,
            // Si no hay errores, el porcentaje es válido; CERO nunca llega a usarse.
            porcentaje_comision_defecto: porcentaje_comision_defecto.unwrap_or(Porcentaje::CERO),
        })
    }

    pub(crate) fn nombre_comercial(&self) -> &str {
        &self.nombre_comercial
    }
    pub(crate) fn nit(&self) -> Option<&str> {
        self.nit.as_deref()
    }
    pub(crate) fn direccion(&self) -> Option<&str> {
        self.direccion.as_deref()
    }
    pub(crate) fn zona_referencia(&self) -> Option<&str> {
        self.zona_referencia.as_deref()
    }
    pub(crate) fn whatsapp(&self) -> Option<&Celular> {
        self.whatsapp.as_ref()
    }
    pub(crate) fn instagram(&self) -> Option<&str> {
        self.instagram.as_deref()
    }
    pub(crate) fn porcentaje_comision_defecto(&self) -> Porcentaje {
        self.porcentaje_comision_defecto
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn valida() -> PropuestaDatosNegocio {
        PropuestaDatosNegocio {
            nombre_comercial: "  Diamante Barbería ".into(),
            nit: Some("900.123.456-7".into()),
            direccion: Some("Cra. 23 # 45-67".into()),
            zona_referencia: Some("   ".into()),
            whatsapp: Some("+57 300 555 0000".into()),
            instagram: Some("@diamante.barberia".into()),
            porcentaje_comision_defecto: 50.0,
        }
    }

    #[test]
    fn normaliza_los_datos_validos() {
        let datos = DatosNegocio::nuevo(valida()).unwrap();
        assert_eq!(datos.nombre_comercial(), "Diamante Barbería");
        assert_eq!(datos.zona_referencia(), None);
        assert_eq!(datos.whatsapp().map(Celular::digitos), Some("3005550000"));
        assert_eq!(datos.instagram(), Some("diamante.barberia"));
        assert_eq!(datos.porcentaje_comision_defecto().centesimas(), 5_000);
    }

    #[test]
    fn informa_todos_los_campos_invalidos_a_la_vez() {
        let propuesta = PropuestaDatosNegocio {
            nombre_comercial: "D".into(),
            nit: Some("12-34".into()),
            whatsapp: Some("2105559917".into()),
            instagram: Some("diamante barberia".into()),
            porcentaje_comision_defecto: 120.0,
            ..valida()
        };
        let ErrorDominio::DatosInvalidos(errores) = DatosNegocio::nuevo(propuesta).unwrap_err();
        let campos: Vec<_> = errores.iter().map(|e| e.campo.as_str()).collect();
        assert_eq!(
            campos,
            [
                "nombre_comercial",
                "nit",
                "whatsapp",
                "instagram",
                "porcentaje_comision_defecto"
            ]
        );
    }

    #[test]
    fn el_nit_sin_digito_de_verificacion_tambien_vale() {
        let propuesta = PropuestaDatosNegocio {
            nit: Some("900123456".into()),
            ..valida()
        };
        assert!(DatosNegocio::nuevo(propuesta).is_ok());
    }
}
