use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::{IdArchivo, IdUsuario};

use crate::dominio::{ClaveArchivo, TipoArchivo, Visibilidad};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevoRegistro {
    pub(crate) clave: ClaveArchivo,
    pub(crate) nombre_original: String,
    pub(crate) tipo: TipoArchivo,
    pub(crate) tamano_bytes: usize,
    pub(crate) sha256: [u8; 32],
    pub(crate) visibilidad: Visibilidad,
    pub(crate) subido_por: Option<IdUsuario>,
    pub(crate) ahora: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistroArchivo {
    pub(crate) id: IdArchivo,
    pub(crate) clave: ClaveArchivo,
    pub(crate) nombre_original: String,
    pub(crate) tipo: TipoArchivo,
    pub(crate) tamano_bytes: usize,
    pub(crate) visibilidad: Visibilidad,
}

#[async_trait]
pub(crate) trait RepositorioArchivos<C: Send>: Send + Sync + fmt::Debug {
    async fn insertar(&self, cx: &mut C, nuevo: &NuevoRegistro) -> Result<IdArchivo, ErrorPersistencia>;
    async fn obtener(&self, cx: &mut C, ids: &[IdArchivo])
    -> Result<Vec<RegistroArchivo>, ErrorPersistencia>;
    async fn eliminar(&self, cx: &mut C, id: IdArchivo) -> Result<Option<ClaveArchivo>, ErrorPersistencia>;
}

/// Contenido de los archivos (volumen en disco en producción, memoria en las pruebas).
#[async_trait]
pub(crate) trait Almacenamiento: Send + Sync + fmt::Debug {
    async fn escribir(&self, clave: &ClaveArchivo, contenido: &[u8]) -> Result<(), String>;
    async fn leer(&self, clave: &ClaveArchivo) -> Result<Option<Vec<u8>>, String>;
    async fn borrar(&self, clave: &ClaveArchivo) -> Result<(), String>;
}
