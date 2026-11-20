#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ErrorDominio {
    #[error("La contraseña debe tener entre 12 y 128 caracteres.")]
    LongitudContrasena,
    #[error("El nombre debe tener entre 2 y 120 caracteres.")]
    LongitudNombre,
}
