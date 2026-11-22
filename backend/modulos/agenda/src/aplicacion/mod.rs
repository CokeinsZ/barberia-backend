//! Casos de uso del módulo, agrupados por audiencia: sitio público, reserva, gestión por enlace
//! y panel del personal.

pub(crate) mod concurrencia;
pub(crate) mod dto;
pub(crate) mod errores;
pub(crate) mod fachada;
pub(crate) mod gestion;
pub(crate) mod manejadores;
pub(crate) mod panel;
pub(crate) mod publico;
pub(crate) mod puertos;
pub(crate) mod recordatorios;
pub(crate) mod reserva;

use std::sync::Arc;

use gestion::Gestion;
use panel::Panel;
use publico::Publico;
use puertos::Puertos;
use recordatorios::Recordatorios;
use reserva::Reserva;

#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) publico: Publico<C>,
    pub(crate) reserva: Reserva<C>,
    pub(crate) gestion: Gestion<C>,
    pub(crate) panel: Panel<C>,
    pub(crate) recordatorios: Recordatorios<C>,
    /// Para construir los manejadores de eventos.
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            publico: Publico { p: p.clone() },
            reserva: Reserva { p: p.clone() },
            gestion: Gestion { p: p.clone() },
            panel: Panel { p: p.clone() },
            recordatorios: Recordatorios { p: p.clone() },
            p,
        }
    }
}
