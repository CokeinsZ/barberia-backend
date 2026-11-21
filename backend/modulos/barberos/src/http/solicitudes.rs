//! Cuerpos y parámetros de entrada. Rechazan campos desconocidos para detectar errores del cliente.

use chrono::{NaiveDate, NaiveTime};
use nucleo::identificadores::{IdBarbero, IdServicio};
use serde::{Deserialize, Deserializer};

use crate::aplicacion::comandos::{SolicitudBloqueo, SolicitudRegistro, SolicitudVisibilidad};
use crate::dominio::{Franja, PropuestaBarbero, PropuestaPrivados};

/// Hora `HH:MM` o `HH:MM:SS`.
fn hora<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
    let texto = String::deserialize(d)?;
    NaiveTime::parse_from_str(&texto, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(&texto, "%H:%M"))
        .map_err(|_| serde::de::Error::custom(format!("hora inválida «{texto}» (use HH:MM)")))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListadoParametros {
    pub(crate) q: Option<String>,
    pub(crate) estado: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FranjaSolicitud {
    dia_semana: u8,
    #[serde(deserialize_with = "hora")]
    inicio: NaiveTime,
    #[serde(deserialize_with = "hora")]
    fin: NaiveTime,
}

impl From<FranjaSolicitud> for Franja {
    fn from(f: FranjaSolicitud) -> Self {
        Franja {
            dia_semana: f.dia_semana,
            inicio: f.inicio,
            fin: f.fin,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VisibilidadSolicitud {
    mostrar_en_landing: bool,
    recibe_citas_en_linea: bool,
    acceso_panel: bool,
}

impl From<VisibilidadSolicitud> for SolicitudVisibilidad {
    fn from(v: VisibilidadSolicitud) -> Self {
        SolicitudVisibilidad {
            mostrar_en_landing: v.mostrar_en_landing,
            recibe_citas_en_linea: v.recibe_citas_en_linea,
            acceso_panel: v.acceso_panel,
        }
    }
}

/// Datos esenciales, comunes al registro y a la edición.
macro_rules! solicitud_datos {
    ($(#[$meta:meta])* $nombre:ident { $($extra:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $nombre {
            tipo_documento: String,
            numero_documento: String,
            nombres: String,
            apellidos: String,
            fecha_nacimiento: Option<NaiveDate>,
            celular: String,
            correo: String,
            especialidad: String,
            anios_experiencia: Option<i64>,
            instagram: Option<String>,
            biografia: Option<String>,
            fecha_ingreso: Option<NaiveDate>,
            tipo_vinculacion: String,
            fin_contrato: Option<NaiveDate>,
            /// Sin valor: la comisión por defecto del negocio (al registrar) o la actual (al editar).
            porcentaje_comision: Option<f64>,
            periodo_liquidacion: Option<String>,
            $($extra)*
        }

        impl $nombre {
            fn propuesta(&self) -> PropuestaBarbero {
                PropuestaBarbero {
                    tipo_documento: self.tipo_documento.clone(),
                    numero_documento: self.numero_documento.clone(),
                    nombres: self.nombres.clone(),
                    apellidos: self.apellidos.clone(),
                    fecha_nacimiento: self.fecha_nacimiento,
                    celular: self.celular.clone(),
                    correo: self.correo.clone(),
                    especialidad: self.especialidad.clone(),
                    anios_experiencia: self.anios_experiencia,
                    instagram: self.instagram.clone(),
                    biografia: self.biografia.clone(),
                    fecha_ingreso: self.fecha_ingreso,
                    tipo_vinculacion: self.tipo_vinculacion.clone(),
                    fin_contrato: self.fin_contrato,
                    porcentaje_comision: self.porcentaje_comision,
                    periodo_liquidacion: self.periodo_liquidacion.clone(),
                }
            }
        }
    };
}

solicitud_datos!(
    /// `POST /barberos`: datos esenciales, servicios, horario y visibilidad.
    RegistroSolicitud {
        #[serde(default)]
        servicios: Vec<i64>,
        #[serde(default)]
        horario: Vec<FranjaSolicitud>,
        visibilidad: Option<VisibilidadSolicitud>,
    }
);

impl From<RegistroSolicitud> for SolicitudRegistro {
    fn from(s: RegistroSolicitud) -> Self {
        SolicitudRegistro {
            propuesta: s.propuesta(),
            servicios: s.servicios.into_iter().map(IdServicio).collect(),
            franjas: s.horario.into_iter().map(Franja::from).collect(),
            visibilidad: s.visibilidad.map(SolicitudVisibilidad::from).unwrap_or_default(),
        }
    }
}

solicitud_datos!(
    /// `PUT /barberos/{id}`: solo los datos esenciales.
    EdicionSolicitud {}
);

impl From<EdicionSolicitud> for PropuestaBarbero {
    fn from(s: EdicionSolicitud) -> Self {
        s.propuesta()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrdenSolicitud {
    pub(crate) barbero_ids: Vec<i64>,
}

impl OrdenSolicitud {
    pub(crate) fn ids(self) -> Vec<IdBarbero> {
        self.barbero_ids.into_iter().map(IdBarbero).collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetiroSolicitud {
    pub(crate) fecha: NaiveDate,
    pub(crate) motivo: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HorarioSolicitud {
    pub(crate) franjas: Vec<FranjaSolicitud>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiciosSolicitud {
    pub(crate) servicio_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AusenciaSolicitud {
    pub(crate) tipo: String,
    pub(crate) desde: NaiveDate,
    pub(crate) hasta: NaiveDate,
    pub(crate) observacion: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BloqueoSolicitud {
    fecha: NaiveDate,
    #[serde(deserialize_with = "hora")]
    desde: NaiveTime,
    #[serde(deserialize_with = "hora")]
    hasta: NaiveTime,
    motivo: String,
    nota: Option<String>,
}

impl From<BloqueoSolicitud> for SolicitudBloqueo {
    fn from(b: BloqueoSolicitud) -> Self {
        SolicitudBloqueo {
            fecha: b.fecha,
            desde: b.desde,
            hasta: b.hasta,
            motivo: b.motivo,
            nota: b.nota,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BloqueosParametros {
    pub(crate) desde: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContactoEmergenciaSolicitud {
    nombre: Option<String>,
    parentesco: Option<String>,
    celular: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrivadosSolicitud {
    direccion: Option<String>,
    ciudad: Option<String>,
    grupo_sanguineo: Option<String>,
    emergencia: Option<ContactoEmergenciaSolicitud>,
    eps: Option<String>,
    fondo_pensiones: Option<String>,
    arl: Option<String>,
    caja_compensacion: Option<String>,
    medio_pago: Option<String>,
    banco: Option<String>,
    tipo_cuenta: Option<String>,
    numero_cuenta: Option<String>,
}

impl From<PrivadosSolicitud> for PropuestaPrivados {
    fn from(s: PrivadosSolicitud) -> Self {
        let emergencia = s.emergencia.unwrap_or(ContactoEmergenciaSolicitud {
            nombre: None,
            parentesco: None,
            celular: None,
        });
        PropuestaPrivados {
            direccion: s.direccion,
            ciudad: s.ciudad,
            grupo_sanguineo: s.grupo_sanguineo,
            emergencia_nombre: emergencia.nombre,
            emergencia_parentesco: emergencia.parentesco,
            emergencia_celular: emergencia.celular,
            eps: s.eps,
            fondo_pensiones: s.fondo_pensiones,
            arl: s.arl,
            caja_compensacion: s.caja_compensacion,
            medio_pago: s.medio_pago,
            banco: s.banco,
            tipo_cuenta: s.tipo_cuenta,
            numero_cuenta: s.numero_cuenta,
        }
    }
}
