//! Paginación de listados: `?pagina=1&por_pagina=20` (máximo 100), según `docs/diseno/api.md`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SolicitudPagina {
    #[serde(default = "primera")]
    pub pagina: u32,
    #[serde(default = "veinte")]
    pub por_pagina: u32,
}

fn primera() -> u32 {
    1
}

fn veinte() -> u32 {
    20
}

impl Default for SolicitudPagina {
    fn default() -> Self {
        Self {
            pagina: 1,
            por_pagina: 20,
        }
    }
}

impl SolicitudPagina {
    pub const MAXIMO_POR_PAGINA: u32 = 100;

    /// Corrige valores fuera de rango en lugar de fallar.
    pub fn normalizada(self) -> Self {
        Self {
            pagina: self.pagina.max(1),
            por_pagina: self.por_pagina.clamp(1, Self::MAXIMO_POR_PAGINA),
        }
    }

    pub fn limite(self) -> i64 {
        i64::from(self.normalizada().por_pagina)
    }

    pub fn desplazamiento(self) -> i64 {
        let n = self.normalizada();
        i64::from(n.pagina - 1) * i64::from(n.por_pagina)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pagina<T> {
    pub datos: Vec<T>,
    pub pagina: u32,
    pub por_pagina: u32,
    pub total: i64,
}

impl<T> Pagina<T> {
    pub fn nueva(datos: Vec<T>, solicitud: SolicitudPagina, total: i64) -> Self {
        let n = solicitud.normalizada();
        Self {
            datos,
            pagina: n.pagina,
            por_pagina: n.por_pagina,
            total,
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn calcula_limite_y_desplazamiento_con_valores_corregidos() {
        let s = SolicitudPagina {
            pagina: 3,
            por_pagina: 20,
        };
        assert_eq!((s.limite(), s.desplazamiento()), (20, 40));
        let fuera = SolicitudPagina {
            pagina: 0,
            por_pagina: 500,
        };
        assert_eq!((fuera.limite(), fuera.desplazamiento()), (100, 0));
    }
}
