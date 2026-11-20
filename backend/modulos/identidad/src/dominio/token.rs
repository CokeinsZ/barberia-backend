use chrono::TimeDelta;

/// Para qué sirve un enlace enviado por correo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropositoToken {
    /// Invitación: crear la contraseña por primera vez.
    Activacion,
    Recuperacion,
}

impl PropositoToken {
    pub(crate) fn vigencia(self) -> TimeDelta {
        match self {
            Self::Activacion => TimeDelta::days(7),
            Self::Recuperacion => TimeDelta::hours(1),
        }
    }

    /// Valor del enum `identidad.proposito_token`.
    pub(crate) fn como_str(self) -> &'static str {
        match self {
            Self::Activacion => "activacion",
            Self::Recuperacion => "recuperacion",
        }
    }

    /// Ruta del frontend que abre el enlace. El token va en el fragmento (`#`): el navegador no
    /// lo envía al servidor, así no queda en logs ni en el encabezado `Referer`.
    pub(crate) fn ruta(self) -> &'static str {
        match self {
            Self::Activacion => "/acceso/activar",
            Self::Recuperacion => "/acceso/restablecer",
        }
    }

    pub(crate) fn enlace(self, url_publica: &str, token: &str) -> String {
        format!(
            "{}{}#token={token}",
            url_publica.trim_end_matches('/'),
            self.ruta()
        )
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_enlace_lleva_el_token_en_el_fragmento() {
        assert_eq!(
            PropositoToken::Recuperacion.enlace("https://diamante.co/", "abc"),
            "https://diamante.co/acceso/restablecer#token=abc"
        );
        assert_eq!(PropositoToken::Activacion.vigencia(), TimeDelta::days(7));
        assert_eq!(PropositoToken::Recuperacion.vigencia(), TimeDelta::hours(1));
    }
}
