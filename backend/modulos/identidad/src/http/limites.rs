use std::net::IpAddr;
use std::time::Duration;

use plataforma::http::ErrorHttp;
use plataforma::seguridad::LimitadorIntentos;

/// Límites de intentos de `docs/diseno/api.md`, sección 1.4. Viven en memoria: con una sola
/// instancia de la API basta; con varias, cada una lleva su cuenta.
#[derive(Debug)]
pub(crate) struct Limites {
    inicio_sesion_por_ip: LimitadorIntentos,
    recuperacion_por_correo: LimitadorIntentos,
    recuperacion_por_ip: LimitadorIntentos,
}

impl Limites {
    pub(crate) fn estandar() -> Self {
        Self {
            inicio_sesion_por_ip: LimitadorIntentos::nuevo(10, Duration::from_secs(15 * 60)),
            recuperacion_por_correo: LimitadorIntentos::nuevo(3, Duration::from_secs(60 * 60)),
            recuperacion_por_ip: LimitadorIntentos::nuevo(10, Duration::from_secs(60 * 60)),
        }
    }

    pub(crate) fn inicio_sesion(&self, ip: Option<IpAddr>) -> Result<(), ErrorHttp> {
        self.inicio_sesion_por_ip
            .intentar(&clave_ip(ip))
            .map_err(ErrorHttp::demasiados_intentos)
    }

    pub(crate) fn recuperacion(&self, ip: Option<IpAddr>, correo: &str) -> Result<(), ErrorHttp> {
        self.recuperacion_por_ip
            .intentar(&clave_ip(ip))
            .map_err(ErrorHttp::demasiados_intentos)?;
        self.recuperacion_por_correo
            .intentar(&correo.trim().to_lowercase())
            .map_err(ErrorHttp::demasiados_intentos)
    }
}

fn clave_ip(ip: Option<IpAddr>) -> String {
    ip.map_or_else(|| "desconocida".to_owned(), |ip| ip.to_string())
}
