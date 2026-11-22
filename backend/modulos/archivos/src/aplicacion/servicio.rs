use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nucleo::identificadores::IdArchivo;
use nucleo::{Reloj, Transacciones};
use plataforma::seguridad::FirmadorUrls;
use sha2::{Digest, Sha256};

use super::puertos::{Almacenamiento, NuevoRegistro, RepositorioArchivos};
use crate::api::{ArchivoGuardado, ArchivoNuevo, Archivos, ErrorArchivos, MetadatosArchivo};
use crate::dominio::{ClaveArchivo, TipoArchivo, Visibilidad, sanear_nombre};

/// Implementación de la fachada [`Archivos`] y de las descargas.
pub(crate) struct ServicioArchivos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) repositorio: Arc<dyn RepositorioArchivos<C>>,
    pub(crate) almacenamiento: Arc<dyn Almacenamiento>,
    pub(crate) firmador: FirmadorUrls,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> std::fmt::Debug for ServicioArchivos<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServicioArchivos").finish_non_exhaustive()
    }
}

/// Archivo listo para entregar por HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Descarga {
    pub(crate) nombre_original: String,
    pub(crate) tipo: TipoArchivo,
    pub(crate) contenido: Vec<u8>,
}

fn mensaje_firma(id: IdArchivo, expira: i64) -> String {
    format!("archivo:{id}:{expira}")
}

impl<C: Send> ServicioArchivos<C> {
    fn validar(nuevo: &ArchivoNuevo) -> Result<TipoArchivo, ErrorArchivos> {
        let campo = nuevo.campo;
        if nuevo.contenido.is_empty() {
            return Err(ErrorArchivos::Vacio { campo });
        }
        if nuevo.contenido.len() > nuevo.tamano_maximo {
            return Err(ErrorArchivos::DemasiadoGrande {
                campo,
                maximo_mb: nuevo.tamano_maximo.div_ceil(1024 * 1024),
            });
        }
        let tipo = TipoArchivo::detectar(&nuevo.contenido)
            .filter(|t| nuevo.tipos_permitidos.contains(t))
            .ok_or_else(|| ErrorArchivos::TipoNoPermitido {
                campo,
                permitidos: TipoArchivo::nombres(nuevo.tipos_permitidos),
            })?;
        if tipo.es_imagen() {
            // Una imagen que no se puede leer se rechaza como tipo no válido.
            let medidas =
                imagesize::blob_size(&nuevo.contenido).map_err(|_| ErrorArchivos::TipoNoPermitido {
                    campo,
                    permitidos: TipoArchivo::nombres(nuevo.tipos_permitidos),
                })?;
            if nuevo.exigir_vertical && medidas.height < medidas.width {
                return Err(ErrorArchivos::NoVertical { campo });
            }
        }
        Ok(tipo)
    }

    /// Descarga de un archivo con URL firmada. La firma se verifica antes que el vencimiento y
    /// antes de tocar la base de datos.
    pub(crate) async fn descargar(
        &self,
        id: IdArchivo,
        expira: i64,
        firma: &str,
    ) -> Result<Descarga, ErrorArchivos> {
        if !self.firmador.verificar(&mensaje_firma(id, expira), firma) {
            return Err(ErrorArchivos::FirmaInvalida);
        }
        if expira < self.reloj.ahora().timestamp() {
            return Err(ErrorArchivos::EnlaceVencido);
        }
        let mut cx = self.transacciones.iniciar().await?;
        let registro = self.repositorio.obtener(&mut cx, &[id]).await?.into_iter().next();
        self.transacciones.confirmar(cx).await?;
        let registro = registro.ok_or(ErrorArchivos::NoEncontrado)?;
        let contenido = self
            .almacenamiento
            .leer(&registro.clave)
            .await
            .map_err(ErrorArchivos::Almacenamiento)?
            .ok_or(ErrorArchivos::NoEncontrado)?;
        Ok(Descarga {
            nombre_original: registro.nombre_original,
            tipo: registro.tipo,
            contenido,
        })
    }

    /// Lectura de un archivo público por su ruta (`/media/publico/…`).
    pub(crate) async fn leer_publico(
        &self,
        ruta: &str,
    ) -> Result<Option<(TipoArchivo, Vec<u8>)>, ErrorArchivos> {
        let Some(clave) = ClaveArchivo::validar(ruta).filter(ClaveArchivo::es_publica) else {
            return Ok(None);
        };
        let Some(tipo) = clave
            .como_str()
            .rsplit_once('.')
            .and_then(|(_, ext)| TipoArchivo::desde_extension(ext))
        else {
            return Ok(None);
        };
        let contenido = self
            .almacenamiento
            .leer(&clave)
            .await
            .map_err(ErrorArchivos::Almacenamiento)?;
        Ok(contenido.map(|c| (tipo, c)))
    }
}

#[async_trait]
impl<C: Send> Archivos<C> for ServicioArchivos<C> {
    async fn guardar(&self, cx: &mut C, nuevo: ArchivoNuevo) -> Result<ArchivoGuardado, ErrorArchivos> {
        let tipo = Self::validar(&nuevo)?;
        let mut aleatorio = [0_u8; 16];
        getrandom::fill(&mut aleatorio).map_err(|e| ErrorArchivos::Almacenamiento(e.to_string()))?;
        let clave = ClaveArchivo::generar(nuevo.visibilidad, nuevo.carpeta, tipo, aleatorio)
            .ok_or_else(|| ErrorArchivos::Almacenamiento(format!("carpeta inválida: {}", nuevo.carpeta)))?;
        let sha256: [u8; 32] = Sha256::digest(&nuevo.contenido).into();
        let nombre_original = sanear_nombre(&nuevo.nombre_original);

        self.almacenamiento
            .escribir(&clave, &nuevo.contenido)
            .await
            .map_err(ErrorArchivos::Almacenamiento)?;
        let registro = NuevoRegistro {
            clave: clave.clone(),
            nombre_original: nombre_original.clone(),
            tipo,
            tamano_bytes: nuevo.contenido.len(),
            sha256,
            visibilidad: nuevo.visibilidad,
            subido_por: nuevo.subido_por,
            ahora: self.reloj.ahora(),
        };
        let id = match self.repositorio.insertar(cx, &registro).await {
            Ok(id) => id,
            Err(error) => {
                self.borrar_contenido(&clave).await;
                return Err(error.into());
            }
        };
        Ok(ArchivoGuardado {
            id,
            clave,
            tipo,
            tamano_bytes: nuevo.contenido.len(),
            nombre_original,
        })
    }

    async fn eliminar(&self, cx: &mut C, id: IdArchivo) -> Result<Option<ClaveArchivo>, ErrorArchivos> {
        Ok(self.repositorio.eliminar(cx, id).await?)
    }

    async fn borrar_contenido(&self, clave: &ClaveArchivo) {
        if let Err(error) = self.almacenamiento.borrar(clave).await {
            tracing::warn!(clave = %clave, error, "no se pudo borrar el contenido del archivo");
        }
    }

    async fn metadatos(&self, cx: &mut C, ids: &[IdArchivo]) -> Result<Vec<MetadatosArchivo>, ErrorArchivos> {
        Ok(self
            .repositorio
            .obtener(cx, ids)
            .await?
            .into_iter()
            .map(|r| MetadatosArchivo {
                id: r.id,
                nombre_original: r.nombre_original,
                tipo: r.tipo,
                tamano_bytes: r.tamano_bytes,
                visibilidad: r.visibilidad,
            })
            .collect())
    }

    async fn urls_publicas(
        &self,
        cx: &mut C,
        ids: &[IdArchivo],
    ) -> Result<HashMap<IdArchivo, String>, ErrorArchivos> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .repositorio
            .obtener(cx, ids)
            .await?
            .into_iter()
            .filter(|r| r.visibilidad == Visibilidad::Publico)
            .map(|r| (r.id, r.clave.url_publica()))
            .collect())
    }

    fn url_firmada(&self, id: IdArchivo, vigencia: Duration) -> String {
        let segundos = i64::try_from(vigencia.as_secs()).unwrap_or(300);
        let expira = self.reloj.ahora().timestamp() + segundos;
        let firma = self.firmador.firmar(&mensaje_firma(id, expira));
        format!("/api/v1/archivos/{id}?exp={expira}&firma={firma}")
    }
}
