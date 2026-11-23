//! Casos de uso del módulo, cada uno con su política de autorización.

mod casos;
pub(crate) mod dto;
pub(crate) mod errores;
mod fachada;
pub(crate) mod puertos;
pub(crate) mod titular;

use std::sync::Arc;

pub(crate) use casos::{
    BuscarClientes, ConfiguracionSegmentos, CrearCliente, EditarCliente, FichaCliente, ListarClientes,
    SolicitudCliente,
};
pub(crate) use fachada::FachadaClientes;
use puertos::Puertos;

pub(crate) const MODULO: &str = "clientes";
pub(crate) const ENTIDAD: &str = "cliente";

#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) buscar: BuscarClientes<C>,
    pub(crate) listar: ListarClientes<C>,
    pub(crate) ficha: FichaCliente<C>,
    pub(crate) crear: CrearCliente<C>,
    pub(crate) editar: EditarCliente<C>,
    pub(crate) titular: titular::Titular<C>,
    pub(crate) segmentos: ConfiguracionSegmentos<C>,
    pub(crate) fachada: Arc<FachadaClientes<C>>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            buscar: BuscarClientes { p: p.clone() },
            listar: ListarClientes { p: p.clone() },
            ficha: FichaCliente { p: p.clone() },
            crear: CrearCliente { p: p.clone() },
            editar: EditarCliente { p: p.clone() },
            titular: titular::Titular { p: p.clone() },
            segmentos: ConfiguracionSegmentos { p: p.clone() },
            fachada: Arc::new(FachadaClientes { p }),
        }
    }
}
