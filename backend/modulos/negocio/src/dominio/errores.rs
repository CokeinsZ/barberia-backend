use nucleo::ErrorDeCampo;

/// Error de las reglas del dominio.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ErrorDominio {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
}

/// Acumula errores de campo para informarlos todos de una vez.
#[derive(Debug, Default)]
pub(crate) struct Validacion(Vec<ErrorDeCampo>);

impl Validacion {
    pub(crate) fn error(&mut self, campo: &str, mensaje: impl Into<String>) {
        self.0.push(ErrorDeCampo::nuevo(campo, mensaje));
    }

    pub(crate) fn terminar<T>(self, valor: impl FnOnce() -> T) -> Result<T, ErrorDominio> {
        if self.0.is_empty() {
            Ok(valor())
        } else {
            Err(ErrorDominio::DatosInvalidos(self.0))
        }
    }
}

/// Texto opcional: vacío o solo espacios cuenta como ausente.
pub(crate) fn texto_opcional(texto: Option<String>) -> Option<String> {
    texto.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}
