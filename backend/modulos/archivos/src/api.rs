//! Fachada pública del módulo `archivos`.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use nucleo::identificadores::{IdArchivo, IdUsuario};
use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo, ErrorPersistencia};

pub use crate::dominio::{ClaveArchivo, TipoArchivo, Visibilidad};

/// Un archivo para guardar, con las reglas que fija el módulo dueño.
#[derive(Debug, Clone)]
pub struct ArchivoNuevo {
    /// Campo del formulario, para señalar el error junto a él.
    pub campo: &'static str,
    pub contenido: Vec<u8>,
    pub nombre_original: String,
    pub visibilidad: Visibilidad,
    /// Subcarpeta dentro de `publico/` o `privado/`, p. ej. `barberos/fotos`.
    pub carpeta: &'static str,
    pub tipos_permitidos: &'static [TipoArchivo],
    pub tamano_maximo: usize,
    /// Si es `true`, la imagen debe ser vertical (alto ≥ ancho).
    pub exigir_vertical: bool,
    pub subido_por: Option<IdUsuario>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivoGuardado {
    pub id: IdArchivo,
    pub clave: ClaveArchivo,
    pub tipo: TipoArchivo,
    pub tamano_bytes: usize,
    pub nombre_original: String,
}

/// Datos de un archivo ya guardado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadatosArchivo {
    pub id: IdArchivo,
    pub nombre_original: String,
    pub tipo: TipoArchivo,
    pub tamano_bytes: usize,
    pub visibilidad: Visibilidad,
}

/// Lo que los demás módulos pueden hacer con los archivos. Las operaciones con `contexto`
/// escriben dentro de la transacción de quien llama (ADR 0005).
#[async_trait]
pub trait Archivos<C: Send>: Send + Sync + fmt::Debug {
    /// Valida y guarda: escribe el contenido en el almacenamiento y registra los metadatos.
    /// Si la transacción se revierte, el contenido queda huérfano y lo borra la limpieza.
    async fn guardar(&self, contexto: &mut C, nuevo: ArchivoNuevo) -> Result<ArchivoGuardado, ErrorArchivos>;

    /// Borra el registro. Devuelve la clave para borrar el contenido **después** de confirmar
    /// la transacción, con [`Archivos::borrar_contenido`].
    async fn eliminar(&self, contexto: &mut C, id: IdArchivo) -> Result<Option<ClaveArchivo>, ErrorArchivos>;

    /// Borra el contenido (mejor esfuerzo: un fallo queda en los logs).
    async fn borrar_contenido(&self, clave: &ClaveArchivo);

    async fn metadatos(
        &self,
        contexto: &mut C,
        ids: &[IdArchivo],
    ) -> Result<Vec<MetadatosArchivo>, ErrorArchivos>;

    /// URL públicas (`/media/publico/…`) de los archivos públicos de la lista.
    async fn urls_publicas(
        &self,
        contexto: &mut C,
        ids: &[IdArchivo],
    ) -> Result<HashMap<IdArchivo, String>, ErrorArchivos>;

    /// URL de descarga firmada, válida durante `vigencia`.
    fn url_firmada(&self, id: IdArchivo, vigencia: Duration) -> String;
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorArchivos {
    #[error("Adjunta un archivo.")]
    Vacio { campo: &'static str },
    #[error("El archivo supera el tamaño máximo de {maximo_mb} MB.")]
    DemasiadoGrande { campo: &'static str, maximo_mb: usize },
    #[error("Tipo de archivo no permitido. Usa {permitidos}.")]
    TipoNoPermitido { campo: &'static str, permitidos: String },
    #[error("La foto debe ser vertical (más alta que ancha).")]
    NoVertical { campo: &'static str },
    #[error("El enlace de descarga venció. Vuelve a pedirlo.")]
    EnlaceVencido,
    #[error("El enlace de descarga no es válido.")]
    FirmaInvalida,
    #[error("El archivo no existe.")]
    NoEncontrado,
    #[error("Error del almacenamiento de archivos: {0}")]
    Almacenamiento(String),
    #[error("Error de persistencia: {0}")]
    Persistencia(#[from] ErrorPersistencia),
}

impl ErrorDeAplicacion for ErrorArchivos {
    fn categoria(&self) -> Categoria {
        match self {
            ErrorArchivos::Vacio { .. }
            | ErrorArchivos::DemasiadoGrande { .. }
            | ErrorArchivos::TipoNoPermitido { .. }
            | ErrorArchivos::NoVertical { .. } => Categoria::Validacion,
            ErrorArchivos::EnlaceVencido => Categoria::Vencido,
            ErrorArchivos::FirmaInvalida => Categoria::Prohibido,
            ErrorArchivos::NoEncontrado => Categoria::NoEncontrado,
            ErrorArchivos::Almacenamiento(_) | ErrorArchivos::Persistencia(_) => Categoria::Interno,
        }
    }

    fn codigo(&self) -> &'static str {
        match self {
            ErrorArchivos::Vacio { .. }
            | ErrorArchivos::DemasiadoGrande { .. }
            | ErrorArchivos::TipoNoPermitido { .. }
            | ErrorArchivos::NoVertical { .. } => "validacion",
            ErrorArchivos::EnlaceVencido => "enlace_vencido",
            ErrorArchivos::FirmaInvalida => "firma_invalida",
            ErrorArchivos::NoEncontrado => "no_encontrado",
            ErrorArchivos::Almacenamiento(_) | ErrorArchivos::Persistencia(_) => "error_interno",
        }
    }

    fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
        match self {
            ErrorArchivos::Vacio { campo }
            | ErrorArchivos::DemasiadoGrande { campo, .. }
            | ErrorArchivos::TipoNoPermitido { campo, .. }
            | ErrorArchivos::NoVertical { campo } => vec![ErrorDeCampo::nuevo(*campo, self.to_string())],
            _ => Vec::new(),
        }
    }
}
