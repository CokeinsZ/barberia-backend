//! Piezas HTTP compartidas: errores en formato *Problem Details* (RFC 9457), extractores de
//! entrada que responden en ese mismo formato y extracción del actor autenticado.

mod cookies;
mod endurecimiento;
mod entrada;
mod formulario;
mod ip;
mod metricas;
mod origen;
mod problema;
mod sesion;

pub use cookies::{TokenSesion, cookie_sesion, cookie_sesion_vencida};
pub use endurecimiento::{
    ConfiguracionLimites, LimitesDeUso, RutaAtendida, anotar_ruta, cabeceras_de_seguridad, hsts, limitar_uso,
};
pub use entrada::{ANIO_MAXIMO, ANIO_MINIMO, Entrada, Parametros, reemplazar_nulos, revisar_texto};
pub use formulario::{ArchivoSubido, Formulario};
pub use ip::{ConfianzaProxy, IpCliente};
pub use metricas::{Latencia, MetricasHttp, ResumenHttp, ResumenRuta, medir};
pub use origen::{OrigenesPermitidos, verificar_origen};
pub use problema::{ErrorHttp, estado_de};
pub use sesion::{ActorAutenticado, COOKIE_SESION, Sesiones, SinSesiones, ValidadorSesion};

/// Manejador para las rutas que no existen.
pub async fn ruta_no_encontrada() -> ErrorHttp {
    ErrorHttp::ruta_no_encontrada()
}
