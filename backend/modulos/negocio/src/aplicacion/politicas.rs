//! Quién puede ejecutar cada caso de uso (heredero del `AbilityFactory` del proyecto de
//! referencia). El servidor las evalúa siempre, aunque la interfaz oculte el botón.

use nucleo::Actor;

use super::errores::ErrorNegocio;

pub(crate) fn solo_administrador(actor: &Actor) -> Result<(), ErrorNegocio> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorNegocio::Prohibido)
    }
}

pub(crate) fn solo_personal(actor: &Actor) -> Result<(), ErrorNegocio> {
    if actor.es_personal() {
        Ok(())
    } else {
        Err(ErrorNegocio::Prohibido)
    }
}
