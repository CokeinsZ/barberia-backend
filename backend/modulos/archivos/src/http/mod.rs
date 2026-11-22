//! `GET /api/v1/archivos/{id}?exp=&firma=` (descarga privada con URL firmada) y
//! `GET /media/{*ruta}` (fotos públicas, con caché de un año).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use nucleo::identificadores::IdArchivo;
use plataforma::http::{ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::ServicioArchivos;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParametrosDescarga {
    exp: i64,
    firma: String,
}

pub(crate) fn rutas_api<C: Send + 'static>(servicio: Arc<ServicioArchivos<C>>) -> Router {
    Router::new()
        .route("/archivos/{id}", get(descargar::<C>))
        .with_state(servicio)
}

pub(crate) fn rutas_media<C: Send + 'static>(servicio: Arc<ServicioArchivos<C>>) -> Router {
    Router::new()
        .route("/media/{*ruta}", get(publico::<C>))
        .with_state(servicio)
}

/// No requiere sesión: la URL firmada es la credencial, y vence en minutos.
async fn descargar<C: Send + 'static>(
    State(servicio): State<Arc<ServicioArchivos<C>>>,
    id: Result<Path<i64>, PathRejection>,
    Parametros(p): Parametros<ParametrosDescarga>,
) -> Result<Response, ErrorHttp> {
    let Path(id) = id.map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    let descarga = servicio.descargar(IdArchivo(id), p.exp, &p.firma).await?;
    let mut respuesta = Body::from(descarga.contenido).into_response();
    let cabeceras = respuesta.headers_mut();
    cabeceras.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(descarga.tipo.mime()),
    );
    // Siempre como descarga: el contenido lo subió una persona y nunca se interpreta en el sitio.
    if let Ok(valor) = HeaderValue::from_str(&disposicion(&descarga.nombre_original)) {
        cabeceras.insert(header::CONTENT_DISPOSITION, valor);
    }
    cabeceras.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    cabeceras.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(respuesta)
}

async fn publico<C: Send + 'static>(
    State(servicio): State<Arc<ServicioArchivos<C>>>,
    Path(ruta): Path<String>,
) -> Result<Response, ErrorHttp> {
    let Some((tipo, contenido)) = servicio.leer_publico(&ruta).await? else {
        return Ok(ErrorHttp::ruta_no_encontrada().into_response());
    };
    let mut respuesta = (StatusCode::OK, Body::from(contenido)).into_response();
    let cabeceras = respuesta.headers_mut();
    cabeceras.insert(header::CONTENT_TYPE, HeaderValue::from_static(tipo.mime()));
    // Las claves son aleatorias y nunca se reutilizan: se pueden guardar en caché un año.
    cabeceras.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    cabeceras.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(respuesta)
}

/// `attachment` con el nombre en ASCII y en UTF-8 (RFC 6266 y 5987).
fn disposicion(nombre: &str) -> String {
    let ascii: String = nombre
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let codificado: String = nombre
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{codificado}")
}

#[cfg(test)]
mod pruebas {
    use super::disposicion;

    #[test]
    fn el_nombre_de_descarga_es_seguro() {
        assert_eq!(
            disposicion("cédula \"x\".pdf"),
            "attachment; filename=\"c_dula _x_.pdf\"; filename*=UTF-8''c%C3%A9dula%20%22x%22.pdf"
        );
    }
}
