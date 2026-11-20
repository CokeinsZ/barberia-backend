use std::time::Duration;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use nucleo::{Categoria, ErrorDeAplicacion, ErrorDeCampo};
use serde::Serialize;

/// Error de una petición HTTP. Se responde como `application/problem+json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorHttp {
    estado: StatusCode,
    codigo: &'static str,
    titulo: String,
    detalle: Option<String>,
    errores: Vec<ErrorDeCampo>,
    reintentar_en: Option<u64>,
}

/// Estado HTTP de cada categoría de error (tabla de `docs/diseno/api.md`, sección 1.3).
pub fn estado_de(categoria: Categoria) -> StatusCode {
    match categoria {
        Categoria::Validacion => StatusCode::UNPROCESSABLE_ENTITY,
        Categoria::NoAutenticado => StatusCode::UNAUTHORIZED,
        Categoria::Prohibido => StatusCode::FORBIDDEN,
        Categoria::NoEncontrado => StatusCode::NOT_FOUND,
        Categoria::Conflicto => StatusCode::CONFLICT,
        Categoria::Vencido => StatusCode::GONE,
        Categoria::DemasiadosIntentos => StatusCode::TOO_MANY_REQUESTS,
        Categoria::Interno => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl ErrorHttp {
    pub fn nuevo(estado: StatusCode, codigo: &'static str, titulo: impl Into<String>) -> Self {
        Self {
            estado,
            codigo,
            titulo: titulo.into(),
            detalle: None,
            errores: Vec::new(),
            reintentar_en: None,
        }
    }

    pub fn con_detalle(mut self, detalle: impl Into<String>) -> Self {
        self.detalle = Some(detalle.into());
        self
    }

    pub fn no_autenticado() -> Self {
        Self::nuevo(
            StatusCode::UNAUTHORIZED,
            "no_autenticado",
            "Inicia sesión para continuar.",
        )
    }

    pub fn solicitud_invalida(detalle: impl Into<String>) -> Self {
        Self::nuevo(
            StatusCode::BAD_REQUEST,
            "solicitud_invalida",
            "La solicitud no es válida.",
        )
        .con_detalle(detalle)
    }

    pub fn ruta_no_encontrada() -> Self {
        Self::nuevo(StatusCode::NOT_FOUND, "no_encontrado", "El recurso no existe.")
    }

    /// 429 con `Retry-After`.
    pub fn demasiados_intentos(espera: Duration) -> Self {
        let mut error = Self::nuevo(
            StatusCode::TOO_MANY_REQUESTS,
            "demasiados_intentos",
            "Demasiados intentos. Espera un momento antes de volver a intentar.",
        );
        error.reintentar_en = Some(espera.as_secs().max(1));
        error
    }

    pub fn interno() -> Self {
        Self::nuevo(
            StatusCode::INTERNAL_SERVER_ERROR,
            "error_interno",
            "Ocurrió un error inesperado. Intenta de nuevo.",
        )
    }

    pub fn estado(&self) -> StatusCode {
        self.estado
    }

    pub fn codigo(&self) -> &'static str {
        self.codigo
    }
}

impl<E: ErrorDeAplicacion> From<E> for ErrorHttp {
    fn from(error: E) -> Self {
        let categoria = error.categoria();
        if categoria == Categoria::Interno {
            // El detalle técnico va a los logs, nunca a la respuesta.
            tracing::error!(codigo = error.codigo(), error = %error, "error interno");
            return ErrorHttp::interno();
        }
        Self {
            estado: estado_de(categoria),
            codigo: error.codigo(),
            titulo: error.to_string(),
            detalle: None,
            errores: error.errores_de_campo(),
            reintentar_en: error.reintentar_en_segundos(),
        }
    }
}

#[derive(Serialize)]
struct Problema<'a> {
    #[serde(rename = "type")]
    tipo: String,
    title: &'a str,
    status: u16,
    codigo: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
    #[serde(skip_serializing_if = "<[ErrorDeCampo]>::is_empty")]
    errores: &'a [ErrorDeCampo],
}

impl IntoResponse for ErrorHttp {
    fn into_response(self) -> Response {
        let cuerpo = Problema {
            tipo: format!("/errores/{}", self.codigo.replace('_', "-")),
            title: &self.titulo,
            status: self.estado.as_u16(),
            codigo: self.codigo,
            detail: self.detalle.as_deref(),
            errores: &self.errores,
        };
        let mut respuesta = (self.estado, Json(cuerpo)).into_response();
        respuesta.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Some(segundos) = self.reintentar_en {
            respuesta
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(segundos));
        }
        respuesta
    }
}

#[cfg(test)]
mod pruebas {
    use http_body_util::BodyExt;

    use super::*;

    #[derive(Debug, thiserror::Error)]
    enum ErrorEjemplo {
        #[error("Revisa los datos.")]
        Datos,
        #[error("se cayó la base de datos con la clave secreta")]
        Interno,
    }

    impl ErrorDeAplicacion for ErrorEjemplo {
        fn categoria(&self) -> Categoria {
            match self {
                ErrorEjemplo::Datos => Categoria::Validacion,
                ErrorEjemplo::Interno => Categoria::Interno,
            }
        }
        fn codigo(&self) -> &'static str {
            match self {
                ErrorEjemplo::Datos => "validacion",
                ErrorEjemplo::Interno => "error_interno",
            }
        }
        fn errores_de_campo(&self) -> Vec<ErrorDeCampo> {
            vec![ErrorDeCampo::nuevo("celular", "Escribe un celular válido.")]
        }
    }

    async fn cuerpo(respuesta: Response) -> serde_json::Value {
        let bytes = respuesta.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn responde_problem_details_con_errores_de_campo() {
        let respuesta = ErrorHttp::from(ErrorEjemplo::Datos).into_response();
        assert_eq!(respuesta.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            respuesta.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let json = cuerpo(respuesta).await;
        assert_eq!(json["codigo"], "validacion");
        assert_eq!(json["type"], "/errores/validacion");
        assert_eq!(json["title"], "Revisa los datos.");
        assert_eq!(json["errores"][0]["campo"], "celular");
    }

    #[test]
    fn demasiados_intentos_incluye_retry_after() {
        let respuesta = ErrorHttp::demasiados_intentos(Duration::from_secs(90)).into_response();
        assert_eq!(respuesta.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(respuesta.headers()[header::RETRY_AFTER], "90");
    }

    #[tokio::test]
    async fn no_filtra_detalles_de_errores_internos() {
        let respuesta = ErrorHttp::from(ErrorEjemplo::Interno).into_response();
        assert_eq!(respuesta.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let texto = cuerpo(respuesta).await.to_string();
        assert!(!texto.contains("clave secreta"));
    }
}
