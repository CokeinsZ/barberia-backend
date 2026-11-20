//! Quién puede ejecutar cada caso de uso. El servidor las evalúa siempre.

use nucleo::Actor;
use nucleo::identificadores::IdUsuario;

use super::errores::ErrorIdentidad;

pub(crate) fn solo_administrador(actor: &Actor) -> Result<(), ErrorIdentidad> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorIdentidad::Prohibido)
    }
}

/// Administradores desde el panel, o el sistema desde la línea de comandos (primer
/// administrador de una instalación nueva).
pub(crate) fn administrador_o_sistema(actor: &Actor) -> Result<(), ErrorIdentidad> {
    if actor.es_administrador() || matches!(actor, Actor::Sistema) {
        Ok(())
    } else {
        Err(ErrorIdentidad::Prohibido)
    }
}

/// Cuenta del personal que actúa (administrador o barbero).
pub(crate) fn cuenta_propia(actor: &Actor) -> Result<IdUsuario, ErrorIdentidad> {
    actor.usuario().ok_or(ErrorIdentidad::Prohibido)
}
