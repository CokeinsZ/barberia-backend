use nucleo::Actor;

use super::errores::ErrorCatalogo;

pub(crate) fn solo_administrador(actor: &Actor) -> Result<(), ErrorCatalogo> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorCatalogo::Prohibido)
    }
}

pub(crate) fn solo_personal(actor: &Actor) -> Result<(), ErrorCatalogo> {
    if actor.es_personal() {
        Ok(())
    } else {
        Err(ErrorCatalogo::Prohibido)
    }
}
