//! Casos de uso del módulo: un archivo por caso de uso, con su política de autorización.

pub(crate) mod comandos;
pub(crate) mod consultas;
pub(crate) mod dto;
pub(crate) mod errores;
pub(crate) mod politicas;
pub(crate) mod puertos;

#[cfg(test)]
mod pruebas;

use std::sync::Arc;

use nucleo::Reloj;

use comandos::{ActualizarDatosNegocio, EliminarDiaEspecial, RegistrarDiaEspecial};
use consultas::{ListarDiasEspeciales, ObtenerDatosNegocio};
use puertos::{RepositorioDiasEspeciales, RepositorioNegocio};

/// Todos los casos de uso del módulo, ya conectados con sus puertos.
#[derive(Debug)]
pub(crate) struct CasosDeUso {
    pub(crate) obtener_datos: ObtenerDatosNegocio,
    pub(crate) actualizar_datos: ActualizarDatosNegocio,
    pub(crate) listar_dias: Arc<ListarDiasEspeciales>,
    pub(crate) registrar_dia: RegistrarDiaEspecial,
    pub(crate) eliminar_dia: EliminarDiaEspecial,
}

impl CasosDeUso {
    pub(crate) fn nuevos(
        negocio: Arc<dyn RepositorioNegocio>,
        dias: Arc<dyn RepositorioDiasEspeciales>,
        reloj: Arc<dyn Reloj>,
    ) -> Self {
        Self {
            obtener_datos: ObtenerDatosNegocio::nuevo(negocio.clone()),
            actualizar_datos: ActualizarDatosNegocio::nuevo(negocio),
            listar_dias: Arc::new(ListarDiasEspeciales::nuevo(dias.clone(), reloj.clone())),
            registrar_dia: RegistrarDiaEspecial::nuevo(dias.clone(), reloj),
            eliminar_dia: EliminarDiaEspecial::nuevo(dias),
        }
    }
}
