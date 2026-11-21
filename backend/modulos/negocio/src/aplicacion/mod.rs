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

use comandos::{
    ActualizarDatosNegocio, EditarDiaEspecial, EliminarDiaEspecial, PrecargarFestivos, PublicarPolitica,
    RegistrarDiaEspecial,
};
use consultas::{
    ConsultarHorarioAtencion, ListarDiasEspeciales, ListarPoliticas, ObtenerDatosNegocio, Parametros,
};
use puertos::Puertos;

/// Módulo y entidades con que este módulo firma sus registros de auditoría.
pub(crate) const MODULO: &str = "negocio";

/// Todos los casos de uso del módulo, ya conectados con sus puertos.
#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) obtener_datos: ObtenerDatosNegocio<C>,
    pub(crate) actualizar_datos: ActualizarDatosNegocio<C>,
    pub(crate) listar_dias: Arc<ListarDiasEspeciales<C>>,
    pub(crate) registrar_dia: RegistrarDiaEspecial<C>,
    pub(crate) editar_dia: EditarDiaEspecial<C>,
    pub(crate) eliminar_dia: EliminarDiaEspecial<C>,
    pub(crate) precargar_festivos: PrecargarFestivos<C>,
    pub(crate) horario_atencion: ConsultarHorarioAtencion<C>,
    pub(crate) listar_politicas: ListarPoliticas<C>,
    pub(crate) publicar_politica: PublicarPolitica<C>,
    pub(crate) parametros: Arc<Parametros<C>>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            obtener_datos: ObtenerDatosNegocio::nuevo(p.clone()),
            actualizar_datos: ActualizarDatosNegocio::nuevo(p.clone()),
            listar_dias: Arc::new(ListarDiasEspeciales::nuevo(p.clone())),
            registrar_dia: RegistrarDiaEspecial::nuevo(p.clone()),
            editar_dia: EditarDiaEspecial::nuevo(p.clone()),
            eliminar_dia: EliminarDiaEspecial::nuevo(p.clone()),
            precargar_festivos: PrecargarFestivos::nuevo(p.clone()),
            horario_atencion: ConsultarHorarioAtencion::nuevo(p.clone()),
            listar_politicas: ListarPoliticas::nuevo(p.clone()),
            publicar_politica: PublicarPolitica::nuevo(p.clone()),
            parametros: Arc::new(Parametros::nuevo(p)),
        }
    }
}
