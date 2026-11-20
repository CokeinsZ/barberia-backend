//! # Contratos entre módulos
//!
//! Eventos que un módulo publica y otros consumen (ver «Eventos entre módulos» en
//! `docs/diseno/api.md`). Gracias a este crate, quien reacciona a un evento no necesita
//! depender del módulo que lo publica, y el grafo de dependencias no tiene ciclos.
//!
//! Reglas: los eventos son datos inmutables, sin lógica; solo se agregan campos de forma
//! compatible; cada evento vive en la carpeta del módulo que lo publica.

pub mod agenda;
pub mod barberos;
pub mod notificaciones;
pub mod ventas;

/// Implementa `EventoDominio` con un nombre estable.
macro_rules! evento {
    ($tipo:ty, $nombre:literal) => {
        impl ::nucleo::eventos::EventoDominio for $tipo {
            fn nombre(&self) -> &'static str {
                $nombre
            }
        }
    };
}
pub(crate) use evento;
