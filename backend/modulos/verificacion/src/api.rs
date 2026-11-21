//! Fachada pública del módulo `verificacion`.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::{Categoria, Celular, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};
use serde::Serialize;

/// Datos que se completan solos después de verificar (RF-RES-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteVerificado {
    pub nombre: String,
    pub correo: Option<String>,
    pub fecha_nacimiento: Option<NaiveDate>,
}

/// Puerto: el cliente registrado con ese celular, si existe (lo implementa `app` con `clientes`).
#[async_trait]
pub trait ClientesConocidos<C: Send>: Send + Sync + fmt::Debug {
    async fn datos_de(
        &self,
        cx: &mut C,
        celular: &Celular,
    ) -> Result<Option<ClienteVerificado>, ErrorPersistencia>;
}

/// Fachada para `agenda`: consume el comprobante dentro de su transacción. Si la transacción se
/// revierte (p. ej. la hora ya se tomó), el comprobante sigue sirviendo para reintentar.
#[async_trait]
pub trait Comprobantes<C: Send>: Send + Sync + fmt::Debug {
    async fn consumir(&self, cx: &mut C, comprobante: &str) -> Result<Celular, ErrorVerificacion>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodigoEnviadoDto {
    pub id: i64,
    pub expira_en: DateTime<Utc>,
    pub reenviar_desde: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificacionDto {
    /// Autoriza crear **una** cita con ese celular.
    pub comprobante: String,
    pub cliente: Option<ClienteVerificado>,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorVerificacion {
    #[error("Revisa los datos marcados.")]
    DatosInvalidos(Vec<ErrorDeCampo>),
    #[error("Espera un momento antes de pedir otro código.")]
    EsperaReenvio { segundos: u64 },
    #[error("El código no es correcto. Te quedan {restantes} intentos.")]
    CodigoIncorrecto { restantes: i16 },
    #[error("El código venció o se invalidó. Pide uno nuevo.")]
    CodigoVencido,
    #[error("La verificación venció o ya se usó. Verifica tu celular de nuevo.")]
    ComprobanteVencido,
    #[error("La verificación no existe.")]
    NoEncontrado,
    #[error("Error interno: {0}")]
    Interno(String),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl ErrorDeAplicacion for ErrorVerificacion {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorVerificacion::DatosInvalidos(_) | ErrorVerificacion::CodigoIncorrecto { .. } => {
                Categoria::Validacion
            }
            ErrorVerificacion::EsperaReenvio { .. } => Categoria::DemasiadosIntentos,
            ErrorVerificacion::CodigoVencido | ErrorVerificacion::ComprobanteVencido => Categoria::Vencido,
            ErrorVerificacion::NoEncontrado => Categoria::NoEncontrado,
            ErrorVerificacion::Interno(_) | ErrorVerificacion::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorVerificacion::DatosInvalidos(_) => "validacion",
            ErrorVerificacion::CodigoIncorrecto { .. } => "codigo_incorrecto",
            ErrorVerificacion::EsperaReenvio { .. } => "demasiados_intentos",
            ErrorVerificacion::CodigoVencido => "codigo_vencido",
            ErrorVerificacion::ComprobanteVencido => "comprobante_vencido",
            ErrorVerificacion::NoEncontrado => "no_encontrado",
            ErrorVerificacion::Interno(_) | ErrorVerificacion::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorVerificacion::DatosInvalidos(e) => e.clone(),
            ErrorVerificacion::CodigoIncorrecto { .. } => {
                vec![ErrorDeCampo::nuevo("codigo", self.to_string())]
            }
            _ => Vec::new(),
        }
    }

    fn reintentar_en_segundos(&self) -> Option<u64> {
        match self {
            ErrorVerificacion::EsperaReenvio { segundos } => Some(*segundos),
            _ => None,
        }
    }
}
