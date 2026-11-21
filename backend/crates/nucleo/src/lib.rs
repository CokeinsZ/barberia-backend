//! # Núcleo
//!
//! Tipos de dominio que comparten todos los módulos. Este crate **no** depende de
//! infraestructura (ni base de datos, ni HTTP): solo de la librería estándar,
//! `chrono`, `serde` y `thiserror`.
//!
//! Debe mantenerse pequeño y estable: si algo solo lo usa un módulo, vive en ese módulo.

pub mod actor;
pub mod errores;
pub mod eventos;
pub mod identificadores;
pub mod paginacion;
pub mod reloj;
pub mod transacciones;
pub mod valores;

pub use actor::{Actor, TipoActor};
pub use errores::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia, ErrorValor, Validacion};
pub use paginacion::{Pagina, SolicitudPagina};
pub use reloj::{Reloj, RelojControlado, RelojSistema, ZONA_HORARIA};
pub use transacciones::{SinTransaccion, Transacciones};
pub use valores::{Celular, Correo, Dinero, Porcentaje};

// Permite que la macro `identificador!` use la ruta `::nucleo::…` también dentro de este crate.
extern crate self as nucleo;

#[doc(hidden)]
pub use serde as __serde;
