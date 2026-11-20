//! Piezas HTTP compartidas: errores en formato *Problem Details* (RFC 9457), extractores de
//! entrada que responden en ese mismo formato y extracción del actor autenticado.

mod entrada;
mod problema;
mod sesion;

pub use entrada::{Entrada, Parametros};
pub use problema::{ErrorHttp, estado_de};
pub use sesion::{ActorAutenticado, COOKIE_SESION, Sesiones, SinSesiones, ValidadorSesion};

/// Manejador para las rutas que no existen.
pub async fn ruta_no_encontrada() -> ErrorHttp {
    ErrorHttp::ruta_no_encontrada()
}
