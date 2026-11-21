//! Quién puede ejecutar cada caso de uso (RN-23, RN-26, RNF-SEG-04).

use nucleo::Actor;
use nucleo::identificadores::IdBarbero;

use super::errores::ErrorBarberos;

pub(crate) fn solo_administrador(actor: &Actor) -> Result<(), ErrorBarberos> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorBarberos::Prohibido)
    }
}

/// El administrador, o el barbero sobre sí mismo. Un barbero que pide los datos de otro recibe
/// «no existe» (404), para no revelar nada.
pub(crate) fn administrador_o_propio(actor: &Actor, barbero: IdBarbero) -> Result<(), ErrorBarberos> {
    match actor {
        Actor::Administrador { .. } => Ok(()),
        Actor::Barbero { barbero: propio, .. } if *propio == barbero => Ok(()),
        Actor::Barbero { .. } => Err(ErrorBarberos::NoEncontrado),
        _ => Err(ErrorBarberos::Prohibido),
    }
}

/// El barbero de la sesión (`GET /yo`).
pub(crate) fn barbero_propio(actor: &Actor) -> Result<IdBarbero, ErrorBarberos> {
    match actor {
        Actor::Barbero { barbero, .. } => Ok(*barbero),
        _ => Err(ErrorBarberos::Prohibido),
    }
}
