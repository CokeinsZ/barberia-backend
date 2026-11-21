//! Casos de uso del módulo: un archivo por caso de uso, con su política de autorización.

pub(crate) mod comandos;
pub(crate) mod consultas;
pub(crate) mod dto;
pub(crate) mod errores;
mod fachada;
pub(crate) mod politicas;
pub(crate) mod puertos;

#[cfg(test)]
mod pruebas;

use std::sync::Arc;

use comandos::{CambiarEstadoServicio, CrearServicio, EditarServicio, ReemplazarComponentes};
use consultas::{ListarCategorias, ListarServicios, ObtenerServicio};
pub(crate) use fachada::Fachada;
use puertos::Puertos;

pub(crate) const MODULO: &str = "catalogo";
pub(crate) const ENTIDAD: &str = "servicio";

#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) listar_categorias: ListarCategorias<C>,
    pub(crate) listar_servicios: ListarServicios<C>,
    pub(crate) obtener_servicio: ObtenerServicio<C>,
    pub(crate) crear_servicio: CrearServicio<C>,
    pub(crate) editar_servicio: EditarServicio<C>,
    pub(crate) cambiar_estado: CambiarEstadoServicio<C>,
    pub(crate) reemplazar_componentes: ReemplazarComponentes<C>,
    pub(crate) fachada: Arc<Fachada<C>>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            listar_categorias: ListarCategorias::nuevo(p.clone()),
            listar_servicios: ListarServicios::nuevo(p.clone()),
            obtener_servicio: ObtenerServicio::nuevo(p.clone()),
            crear_servicio: CrearServicio::nuevo(p.clone()),
            editar_servicio: EditarServicio::nuevo(p.clone()),
            cambiar_estado: CambiarEstadoServicio::nuevo(p.clone()),
            reemplazar_componentes: ReemplazarComponentes::nuevo(p.clone()),
            fachada: Arc::new(Fachada::nueva(p)),
        }
    }
}
