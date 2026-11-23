use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use nucleo::ErrorDeCampo;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::ErrorHttp;

/// Años que acepta cualquier fecha que entra a la API. Fuera de este rango una fecha solo puede
/// ser un error o un ataque: los cálculos de calendario se desbordan (chrono entra en pánico al
/// sumar un día a `+262142-12-31`) y PostgreSQL rechaza años que no existen.
pub const ANIO_MINIMO: i64 = 1900;
pub const ANIO_MAXIMO: i64 = 2999;

/// Revisión común de todo texto que llega en un cuerpo JSON, en la URL o en un formulario:
///
/// - sin el carácter nulo (`\u0000`): PostgreSQL no lo admite en un texto y la consulta fallaría
///   con un `500`;
/// - toda fecha (`AAAA-MM-DD`, `AAAA-MM` o un instante ISO 8601 que empieza así) entre
///   [`ANIO_MINIMO`] y [`ANIO_MAXIMO`].
///
/// Así ninguna ruta, ni las que se agreguen después, depende de que su módulo se acuerde de
/// validarlo.
pub fn revisar_texto(campo: &str, texto: &str) -> Option<ErrorDeCampo> {
    if texto.contains('\0') {
        return Some(ErrorDeCampo::nuevo(
            campo,
            "Tiene un carácter que no se permite (nulo).",
        ));
    }
    match anio_de_fecha(texto) {
        Some(anio) if !(ANIO_MINIMO..=ANIO_MAXIMO).contains(&anio) => Some(ErrorDeCampo::nuevo(
            campo,
            format!("La fecha debe estar entre los años {ANIO_MINIMO} y {ANIO_MAXIMO}."),
        )),
        _ => None,
    }
}

/// Cambia el carácter nulo por `�` en cada texto y clave de un JSON que se guarda tal cual
/// (PostgreSQL no lo admite ni en `text` ni en `jsonb`). Es para las cargas que no se pueden
/// rechazar, como los webhooks de Meta: si se responden con error, Meta los reintenta por días.
pub fn reemplazar_nulos(valor: &mut Value) {
    match valor {
        Value::String(texto) => {
            if texto.contains('\0') {
                *texto = texto.replace('\0', "\u{FFFD}");
            }
        }
        Value::Array(elementos) => elementos.iter_mut().for_each(reemplazar_nulos),
        Value::Object(campos) => {
            if campos.keys().any(|clave| clave.contains('\0')) {
                for (clave, mut hijo) in std::mem::take(campos) {
                    reemplazar_nulos(&mut hijo);
                    campos.insert(clave.replace('\0', "\u{FFFD}"), hijo);
                }
            } else {
                campos.values_mut().for_each(reemplazar_nulos);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Año de un texto con forma de fecha: `[+-]AAAA…-MM`, opcionalmente `-DD` y, después, la hora
/// (`T…` o un espacio). Un año con demasiados dígitos cuenta como fuera de rango.
fn anio_de_fecha(texto: &str) -> Option<i64> {
    let (signo, resto) = match texto.as_bytes().first() {
        Some(b'+') => (1, &texto[1..]),
        Some(b'-') => (-1, &texto[1..]),
        _ => (1, texto),
    };
    let digitos = resto.bytes().take_while(u8::is_ascii_digit).count();
    if digitos < 4 {
        return None;
    }
    let dos_digitos = |t: &str, rango: std::ops::RangeInclusive<u32>| {
        t.get(..2)
            .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|d| d.parse::<u32>().ok())
            .is_some_and(|n| rango.contains(&n))
    };
    let tras_anio = resto[digitos..].strip_prefix('-')?;
    if !dos_digitos(tras_anio, 1..=12) {
        return None;
    }
    let tras_mes = &tras_anio[2..];
    if !tras_mes.is_empty() {
        let tras_guion = tras_mes.strip_prefix('-')?;
        if !dos_digitos(tras_guion, 1..=31) {
            return None;
        }
        let tras_dia = &tras_guion[2..];
        if !(tras_dia.is_empty() || tras_dia.starts_with(['T', 't', ' '])) {
            return None;
        }
    }
    Some(
        resto[..digitos]
            .parse::<i64>()
            .map_or(i64::MAX, |anio| signo * anio),
    )
}

/// Revisa cada texto (y cada clave) de un JSON; el campo se nombra con su ruta
/// (`cliente_nuevo.nombre`, `pagos[1].monto`).
fn revisar_json(ruta: &str, valor: &Value, errores: &mut Vec<ErrorDeCampo>) {
    match valor {
        Value::String(texto) => errores.extend(revisar_texto(nombre_de(ruta), texto)),
        Value::Array(elementos) => {
            for (i, elemento) in elementos.iter().enumerate() {
                revisar_json(&format!("{ruta}[{i}]"), elemento, errores);
            }
        }
        Value::Object(campos) => {
            for (clave, elemento) in campos {
                let hija = if ruta.is_empty() {
                    clave.clone()
                } else {
                    format!("{ruta}.{clave}")
                };
                if clave.contains('\0') {
                    errores.extend(revisar_texto(nombre_de(ruta), clave));
                } else {
                    revisar_json(&hija, elemento, errores);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn nombre_de(ruta: &str) -> &str {
    if ruta.is_empty() { "cuerpo" } else { ruta }
}

fn es_json(cabeceras: &HeaderMap) -> bool {
    let Some(tipo) = cabeceras.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let esencia = tipo
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    esencia == "application/json" || (esencia.starts_with("application/") && esencia.ends_with("+json"))
}

/// Cuerpo JSON de la petición. A diferencia de `Json`, si el cuerpo no sirve responde con
/// *Problem Details* (400) en lugar de texto plano, y pasa cada texto por [`revisar_texto`]
/// (422 con los campos).
#[derive(Debug, Clone)]
pub struct Entrada<T>(pub T);

impl<T, S> FromRequest<S> for Entrada<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErrorHttp;

    async fn from_request(peticion: Request, estado: &S) -> Result<Self, Self::Rejection> {
        if !es_json(peticion.headers()) {
            return Err(ErrorHttp::solicitud_invalida(
                "El cuerpo debe enviarse como JSON (Content-Type: application/json).",
            ));
        }
        // `Bytes` respeta el tamaño máximo de la ruta (`DefaultBodyLimit`).
        let bytes = Bytes::from_request(peticion, estado).await.map_err(|rechazo| {
            if rechazo.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ErrorHttp::nuevo(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "cuerpo_demasiado_grande",
                    "La solicitud es demasiado grande.",
                )
            } else {
                ErrorHttp::solicitud_invalida(rechazo.body_text())
            }
        })?;
        let valor = match Json::<T>::from_bytes(&bytes) {
            Ok(Json(valor)) => valor,
            Err(JsonRejection::JsonSyntaxError(_)) => {
                return Err(ErrorHttp::solicitud_invalida("El JSON está mal formado."));
            }
            Err(otro) => return Err(ErrorHttp::solicitud_invalida(otro.body_text())),
        };
        // El cuerpo ya se leyó como `T` (con sus reglas: campos desconocidos, repetidos…); la
        // revisión de los textos se hace sobre el mismo JSON.
        if let Ok(arbol) = serde_json::from_slice::<Value>(&bytes) {
            let mut errores = Vec::new();
            revisar_json("", &arbol, &mut errores);
            if !errores.is_empty() {
                return Err(ErrorHttp::validacion(errores));
            }
        }
        Ok(Self(valor))
    }
}

/// Parámetros de la URL (`?desde=…&hasta=…`) con errores en formato *Problem Details* y cada
/// valor revisado con [`revisar_texto`].
#[derive(Debug, Clone)]
pub struct Parametros<T>(pub T);

impl<T, S> FromRequestParts<S> for Parametros<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErrorHttp;

    async fn from_request_parts(partes: &mut Parts, estado: &S) -> Result<Self, Self::Rejection> {
        let valor = Query::<T>::from_request_parts(partes, estado)
            .await
            .map(|Query(valor)| valor)
            .map_err(|rechazo| ErrorHttp::solicitud_invalida(rechazo.body_text()))?;
        if let Ok(Query(pares)) = Query::<Vec<(String, String)>>::try_from_uri(&partes.uri) {
            let errores: Vec<ErrorDeCampo> = pares
                .iter()
                .filter_map(|(clave, texto)| {
                    revisar_texto(clave, clave).or_else(|| revisar_texto(clave, texto))
                })
                .collect();
            if !errores.is_empty() {
                return Err(ErrorHttp::validacion(errores));
            }
        }
        Ok(Self(valor))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn reconoce_fechas_y_su_anio() {
        assert_eq!(anio_de_fecha("2026-11-22"), Some(2026));
        assert_eq!(anio_de_fecha("2026-11"), Some(2026));
        assert_eq!(anio_de_fecha("2026-11-22T10:00:00-05:00"), Some(2026));
        assert_eq!(anio_de_fecha("+262142-12-31"), Some(262_142));
        assert_eq!(anio_de_fecha("-262143-01-01"), Some(-262_143));
        assert_eq!(anio_de_fecha("0001-01-01"), Some(1));
        assert_eq!(anio_de_fecha("99999999999999999999-01-01"), Some(i64::MAX));
        // No son fechas: se dejan pasar (si el campo era una fecha, su tipo los rechaza).
        for texto in [
            "",
            "Corte clásico",
            "3001234567",
            "300-123-4567",
            "2026-13-01",
            "2026-11-32",
            "2026-1-01",
            "2026-11-22x",
            "1020-30-40",
            "12-11-2026",
            "2026",
        ] {
            assert_eq!(anio_de_fecha(texto), None, "{texto}");
        }
    }

    #[test]
    fn rechaza_el_nulo_y_las_fechas_fuera_de_rango() {
        assert!(revisar_texto("nombre", "Ana Ruiz").is_none());
        assert!(revisar_texto("fecha", "2026-11-22").is_none());
        assert!(revisar_texto("fecha", "1900-01-01").is_none());
        assert!(revisar_texto("fecha", "2999-12-31").is_none());
        for (texto, mensaje) in [
            ("Ana\0", "nulo"),
            ("1899-12-31", "1900 y 2999"),
            ("3000-01-01", "1900 y 2999"),
            ("+262142-12-31T10:00:00Z", "1900 y 2999"),
        ] {
            let error = revisar_texto("campo", texto).unwrap();
            assert_eq!(error.campo, "campo");
            assert!(error.mensaje.contains(mensaje), "{texto}: {}", error.mensaje);
        }
    }

    #[test]
    fn reemplaza_el_nulo_en_textos_y_claves() {
        let mut carga = serde_json::json!({ "a\u{0}": ["x\u{0}y", { "b": "\u{0}" }], "n": 1 });
        reemplazar_nulos(&mut carga);
        assert_eq!(
            carga,
            serde_json::json!({ "a\u{FFFD}": ["x\u{FFFD}y", { "b": "\u{FFFD}" }], "n": 1 })
        );
    }

    #[test]
    fn nombra_cada_campo_con_su_ruta() {
        let cuerpo = serde_json::json!({
            "nombre": "Ana\u{0}",
            "cliente": { "fecha_nacimiento": "0001-01-01" },
            "pagos": [{ "monto": 1 }, { "nota": "x\u{0}" }],
            "bien": "2026-11-22",
        });
        let mut errores = Vec::new();
        revisar_json("", &cuerpo, &mut errores);
        let mut campos: Vec<&str> = errores.iter().map(|e| e.campo.as_str()).collect();
        campos.sort_unstable();
        assert_eq!(campos, ["cliente.fecha_nacimiento", "nombre", "pagos[1].nota"]);
    }
}
