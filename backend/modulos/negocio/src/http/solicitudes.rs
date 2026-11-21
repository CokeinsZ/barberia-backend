//! Cuerpos y parámetros de entrada. Rechazan campos desconocidos para detectar errores del cliente.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;

use crate::aplicacion::comandos::{NuevoDiaEspecial, SolicitudPolitica};
use crate::dominio::entidades::PropuestaDatosNegocio;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActualizarDatosSolicitud {
    nombre_comercial: String,
    #[serde(default)]
    nit: Option<String>,
    #[serde(default)]
    direccion: Option<String>,
    #[serde(default)]
    zona_referencia: Option<String>,
    #[serde(default)]
    whatsapp: Option<String>,
    #[serde(default)]
    instagram: Option<String>,
    porcentaje_comision_defecto: f64,
}

impl From<ActualizarDatosSolicitud> for PropuestaDatosNegocio {
    fn from(s: ActualizarDatosSolicitud) -> Self {
        Self {
            nombre_comercial: s.nombre_comercial,
            nit: s.nit,
            direccion: s.direccion,
            zona_referencia: s.zona_referencia,
            whatsapp: s.whatsapp,
            instagram: s.instagram,
            porcentaje_comision_defecto: s.porcentaje_comision_defecto,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NuevoDiaSolicitud {
    fecha: NaiveDate,
    nombre: String,
    /// `festivo`, `cerrado` o `cierre_temprano`.
    tipo: String,
    #[serde(default)]
    hora_cierre: Option<NaiveTime>,
}

impl From<NuevoDiaSolicitud> for NuevoDiaEspecial {
    fn from(s: NuevoDiaSolicitud) -> Self {
        Self {
            fecha: s.fecha,
            nombre: s.nombre,
            tipo: s.tipo,
            hora_cierre: s.hora_cierre,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RangoSolicitud {
    pub(crate) desde: Option<NaiveDate>,
    pub(crate) hasta: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NuevaPoliticaSolicitud {
    /// `tratamiento_datos`, `cancelacion` o `terminos`.
    tipo: String,
    version: String,
    url: String,
    #[serde(default)]
    vigente_desde: Option<DateTime<Utc>>,
}

impl From<NuevaPoliticaSolicitud> for SolicitudPolitica {
    fn from(s: NuevaPoliticaSolicitud) -> Self {
        Self {
            tipo: s.tipo,
            version: s.version,
            url: s.url,
            vigente_desde: s.vigente_desde,
        }
    }
}
