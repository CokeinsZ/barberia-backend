//! Quién ejecuta una acción. Lo usan las políticas de autorización y la auditoría.

use crate::identificadores::{IdBarbero, IdCliente, IdUsuario};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Administrador {
        usuario: IdUsuario,
    },
    Barbero {
        usuario: IdUsuario,
        barbero: IdBarbero,
    },
    /// Cliente sin cuenta: se identifica con un comprobante o con el enlace de su cita.
    Cliente {
        cliente: Option<IdCliente>,
    },
    /// Tareas automáticas del worker.
    Sistema,
}

/// Tipo de actor, igual al enum `nucleo.actor` de la base de datos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoActor {
    Cliente,
    Barbero,
    Administrador,
    Sistema,
}

impl TipoActor {
    /// Valor del enum `nucleo.actor` en PostgreSQL.
    pub fn como_str(self) -> &'static str {
        match self {
            TipoActor::Cliente => "cliente",
            TipoActor::Barbero => "barbero",
            TipoActor::Administrador => "administrador",
            TipoActor::Sistema => "sistema",
        }
    }
}

impl Actor {
    pub fn tipo(&self) -> TipoActor {
        match self {
            Actor::Administrador { .. } => TipoActor::Administrador,
            Actor::Barbero { .. } => TipoActor::Barbero,
            Actor::Cliente { .. } => TipoActor::Cliente,
            Actor::Sistema => TipoActor::Sistema,
        }
    }

    /// Cuenta del personal que actúa, si la hay.
    pub fn usuario(&self) -> Option<IdUsuario> {
        match self {
            Actor::Administrador { usuario } | Actor::Barbero { usuario, .. } => Some(*usuario),
            Actor::Cliente { .. } | Actor::Sistema => None,
        }
    }

    pub fn es_administrador(&self) -> bool {
        matches!(self, Actor::Administrador { .. })
    }

    /// Administrador o barbero.
    pub fn es_personal(&self) -> bool {
        matches!(self, Actor::Administrador { .. } | Actor::Barbero { .. })
    }
}
