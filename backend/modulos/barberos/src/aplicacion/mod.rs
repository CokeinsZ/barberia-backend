//! Casos de uso del módulo, agrupados por funcionalidad (un archivo por grupo), cada uno con su
//! política de autorización.

pub(crate) mod comandos;
pub(crate) mod consultas;
pub(crate) mod dto;
pub(crate) mod errores;
mod fachada;
pub(crate) mod politicas;
pub(crate) mod puertos;

use std::sync::Arc;

use comandos::{
    CambiarFoto, CambiarVisibilidad, EditarBarbero, GestionAusencias, GestionBloqueos, GestionDatosPrivados,
    GestionDocumentos, GestionHorario, GestionServicios, OrdenarEquipo, RegistrarBarbero, RetirarBarbero,
};
use consultas::{ListarBarberos, ObtenerFicha};
pub(crate) use fachada::FachadaEquipo;
use puertos::Puertos;

#[derive(Debug)]
pub(crate) struct CasosDeUso<C: Send> {
    pub(crate) listar: ListarBarberos<C>,
    pub(crate) ficha: ObtenerFicha<C>,
    pub(crate) registrar: RegistrarBarbero<C>,
    pub(crate) editar: EditarBarbero<C>,
    pub(crate) visibilidad: CambiarVisibilidad<C>,
    pub(crate) ordenar: OrdenarEquipo<C>,
    pub(crate) retirar: RetirarBarbero<C>,
    pub(crate) horario: GestionHorario<C>,
    pub(crate) servicios: GestionServicios<C>,
    pub(crate) ausencias: GestionAusencias<C>,
    pub(crate) bloqueos: GestionBloqueos<C>,
    pub(crate) foto: CambiarFoto<C>,
    pub(crate) documentos: GestionDocumentos<C>,
    pub(crate) privados: GestionDatosPrivados<C>,
    pub(crate) equipo: Arc<FachadaEquipo<C>>,
}

impl<C: Send> CasosDeUso<C> {
    pub(crate) fn nuevos(puertos: Puertos<C>) -> Self {
        let p = Arc::new(puertos);
        Self {
            listar: ListarBarberos::nuevo(p.clone()),
            ficha: ObtenerFicha::nuevo(p.clone()),
            registrar: RegistrarBarbero::nuevo(p.clone()),
            editar: EditarBarbero::nuevo(p.clone()),
            visibilidad: CambiarVisibilidad::nuevo(p.clone()),
            ordenar: OrdenarEquipo::nuevo(p.clone()),
            retirar: RetirarBarbero::nuevo(p.clone()),
            horario: GestionHorario::nuevo(p.clone()),
            servicios: GestionServicios::nuevo(p.clone()),
            ausencias: GestionAusencias::nuevo(p.clone()),
            bloqueos: GestionBloqueos::nuevo(p.clone()),
            foto: CambiarFoto::nuevo(p.clone()),
            documentos: GestionDocumentos::nuevo(p.clone()),
            privados: GestionDatosPrivados::nuevo(p.clone()),
            equipo: Arc::new(FachadaEquipo::nueva(p)),
        }
    }
}

/// Declara un caso de uso que solo guarda los puertos compartidos.
macro_rules! caso_de_uso {
    ($(#[$meta:meta])* $nombre:ident) => {
        $(#[$meta])*
        pub(crate) struct $nombre<C: Send> {
            p: std::sync::Arc<crate::aplicacion::puertos::Puertos<C>>,
        }

        impl<C: Send> $nombre<C> {
            pub(crate) fn nuevo(p: std::sync::Arc<crate::aplicacion::puertos::Puertos<C>>) -> Self {
                Self { p }
            }
        }

        impl<C: Send> std::fmt::Debug for $nombre<C> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($nombre))
            }
        }
    };
}
pub(crate) use caso_de_uso;
