//! Consulta de la auditoría.

pub(crate) mod dto;
pub(crate) mod errores;
mod listar_registros;
pub(crate) mod puertos;

pub(crate) use listar_registros::{FiltrosAuditoria, ListarRegistros};
