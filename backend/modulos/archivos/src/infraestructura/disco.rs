//! Almacenamiento en el sistema de archivos (volumen `archivos` de Docker).

use std::io::ErrorKind;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::aplicacion::puertos::Almacenamiento;
use crate::dominio::ClaveArchivo;

#[derive(Debug, Clone)]
pub(crate) struct AlmacenamientoEnDisco {
    raiz: PathBuf,
}

impl AlmacenamientoEnDisco {
    pub(crate) fn nuevo(raiz: PathBuf) -> Self {
        Self { raiz }
    }

    /// La clave ya está validada (sin `..` ni rutas absolutas): no puede salir de la raíz.
    fn ruta(&self, clave: &ClaveArchivo) -> PathBuf {
        self.raiz.join(clave.como_str())
    }
}

#[async_trait]
impl Almacenamiento for AlmacenamientoEnDisco {
    /// Escritura atómica: primero a un temporal en la misma carpeta y luego `rename`, para que
    /// nunca se sirva un archivo a medio escribir.
    async fn escribir(&self, clave: &ClaveArchivo, contenido: &[u8]) -> Result<(), String> {
        let ruta = self.ruta(clave);
        if let Some(carpeta) = ruta.parent() {
            tokio::fs::create_dir_all(carpeta)
                .await
                .map_err(|e| format!("{}: {e}", carpeta.display()))?;
        }
        let temporal = ruta.with_extension("tmp");
        tokio::fs::write(&temporal, contenido)
            .await
            .map_err(|e| format!("{}: {e}", temporal.display()))?;
        tokio::fs::rename(&temporal, &ruta)
            .await
            .map_err(|e| format!("{}: {e}", ruta.display()))
    }

    async fn leer(&self, clave: &ClaveArchivo) -> Result<Option<Vec<u8>>, String> {
        match tokio::fs::read(self.ruta(clave)).await {
            Ok(contenido) => Ok(Some(contenido)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn borrar(&self, clave: &ClaveArchivo) -> Result<(), String> {
        match tokio::fs::remove_file(self.ruta(clave)).await {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}
