//! Formularios `multipart/form-data` (carga de fotos y documentos).

use std::collections::HashMap;

use axum::extract::multipart::MultipartError;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::StatusCode;

use super::ErrorHttp;

/// Archivo recibido, todavía sin validar (el módulo `archivos` valida tipo y tamaño).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivoSubido {
    pub nombre: String,
    pub contenido: Vec<u8>,
}

/// Campos de texto y archivos de un formulario. El tamaño máximo del cuerpo lo fija la ruta
/// con `DefaultBodyLimit`; si se supera, responde 413.
#[derive(Debug, Default)]
pub struct Formulario {
    campos: HashMap<String, String>,
    archivos: HashMap<String, ArchivoSubido>,
}

impl Formulario {
    pub fn campo(&self, nombre: &str) -> Option<&str> {
        self.campos
            .get(nombre)
            .map(String::as_str)
            .filter(|v| !v.trim().is_empty())
    }

    /// Toma el archivo del campo `nombre`.
    pub fn archivo(&mut self, nombre: &str) -> Option<ArchivoSubido> {
        self.archivos.remove(nombre)
    }
}

fn traducir(error: MultipartError) -> ErrorHttp {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ErrorHttp::nuevo(
            StatusCode::PAYLOAD_TOO_LARGE,
            "archivo_demasiado_grande",
            "El archivo supera el tamaño máximo permitido.",
        )
    } else {
        ErrorHttp::solicitud_invalida(error.body_text())
    }
}

impl<S: Send + Sync> FromRequest<S> for Formulario {
    type Rejection = ErrorHttp;

    async fn from_request(peticion: Request, estado: &S) -> Result<Self, Self::Rejection> {
        let mut multipart = Multipart::from_request(peticion, estado)
            .await
            .map_err(|_| ErrorHttp::solicitud_invalida("Envíe el formulario como multipart/form-data."))?;
        let mut formulario = Formulario::default();
        while let Some(campo) = multipart.next_field().await.map_err(traducir)? {
            let nombre = campo.name().unwrap_or_default().to_owned();
            match campo.file_name().map(str::to_owned) {
                Some(nombre_archivo) => {
                    let contenido = campo.bytes().await.map_err(traducir)?.to_vec();
                    formulario.archivos.insert(
                        nombre,
                        ArchivoSubido {
                            nombre: nombre_archivo,
                            contenido,
                        },
                    );
                }
                None => {
                    let texto = campo.text().await.map_err(traducir)?;
                    formulario.campos.insert(nombre, texto);
                }
            }
        }
        Ok(formulario)
    }
}
