//! Casos de uso del módulo: ventas (registro, consulta y anulación) y caja (caja del día,
//! «Lo que llevas hoy», cierres y métodos de pago).

pub(crate) mod caja;
pub(crate) mod dto;
pub(crate) mod errores;
pub(crate) mod puertos;
pub(crate) mod ventas;

use std::sync::Arc;

use caja::Caja;
use puertos::Puertos;
use ventas::Ventas;

#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) ventas: Ventas<C>,
    pub(crate) caja: Caja<C>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            ventas: Ventas { p: p.clone() },
            caja: Caja {
                p: p.clone(),
                ventas: Ventas { p },
            },
        }
    }
}
