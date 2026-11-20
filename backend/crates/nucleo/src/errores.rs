//! Errores compartidos.
//!
//! Cada módulo define sus propios errores con `thiserror` e implementa [`ErrorDeAplicacion`].
//! Así la capa HTTP (en `plataforma`) los traduce a respuestas sin que el dominio conozca HTTP.

use serde::Serialize;

/// Categoría de un error, independiente del transporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Categoria {
    Validacion,
    NoAutenticado,
    Prohibido,
    NoEncontrado,
    Conflicto,
    Vencido,
    DemasiadosIntentos,
    Interno,
}

/// Error de un campo concreto, para mostrarlo junto al campo en la interfaz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorDeCampo {
    pub campo: String,
    pub mensaje: String,
}

impl ErrorDeCampo {
    pub fn nuevo(campo: impl Into<String>, mensaje: impl Into<String>) -> Self {
        Self {
            campo: campo.into(),
            mensaje: mensaje.into(),
        }
    }
}

/// Error que puede salir de un caso de uso. Tiene un código estable (p. ej. `conflicto_horario`)
/// que el frontend puede usar, y un mensaje (`Display`) pensado para la persona usuaria.
pub trait ErrorDeAplicacion: std::error::Error + Send + Sync + 'static {
    fn categoria(&self) -> Categoria;

    fn codigo(&self) -> &'static str;

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        Vec::new()
    }
}

/// Valor inválido al construir un objeto de valor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorValor {
    #[error("El valor en pesos no puede ser negativo ({0}).")]
    DineroNegativo(i64),
    #[error("El valor en pesos es demasiado grande.")]
    Desbordamiento,
    #[error("El porcentaje debe estar entre 0 y 100.")]
    PorcentajeFueraDeRango,
    #[error("Escribe un celular colombiano de 10 dígitos que empiece por 3.")]
    CelularInvalido,
    #[error("Escribe un correo válido, por ejemplo nombre@correo.com.")]
    CorreoInvalido,
}

/// Error de un puerto de persistencia, sin exponer el tipo de error de la base de datos.
/// La traducción desde SQLx vive en `plataforma::base_datos`.
#[derive(Debug, thiserror::Error)]
pub enum ErrorPersistencia {
    /// Violación de una restricción única (`23505`).
    #[error("Ya existe un registro que viola «{restriccion}».")]
    Duplicado { restriccion: String },
    /// Violación de una restricción de exclusión (`23P01`), como dos citas cruzadas.
    #[error("La operación choca con «{restriccion}».")]
    Conflicto { restriccion: String },
    /// Violación de una restricción `CHECK` o de una llave foránea.
    #[error("Un dato viola la restricción «{restriccion}».")]
    Restriccion { restriccion: String },
    #[error("Error de la base de datos: {0}")]
    Otro(String),
}

impl ErrorPersistencia {
    /// Nombre de la restricción violada, si aplica.
    pub fn restriccion(&self) -> Option<&str> {
        match self {
            ErrorPersistencia::Duplicado { restriccion }
            | ErrorPersistencia::Conflicto { restriccion }
            | ErrorPersistencia::Restriccion { restriccion } => Some(restriccion),
            ErrorPersistencia::Otro(_) => None,
        }
    }
}
