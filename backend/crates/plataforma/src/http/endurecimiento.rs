//! Capas transversales de endurecimiento (OWASP ASVS nivel 1): cabeceras de seguridad en cada
//! respuesta, límites de uso por IP y por sesión (`docs/diseno/api.md`, 1.4) y la ruta de cada
//! petición en las trazas sin datos personales ni secretos.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use super::IpCliente;
use super::problema::ErrorHttp;
use super::sesion::{COOKIE_SESION, leer_cookie};
use crate::seguridad::LimitadorIntentos;

/// Cabeceras de seguridad de toda respuesta (ASVS V14.4):
///
/// - `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` y `Referrer-Policy: no-referrer`.
/// - `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'` en la API: sus
///   respuestas son datos (JSON o descargas), nunca páginas que ejecuten algo. Las fotos de
///   `/media` no la llevan (el navegador no mostraría la imagen abierta directamente).
/// - `Cache-Control: no-store` salvo que el manejador fije el suyo (la landing pública y las
///   fotos se guardan un rato): datos del panel y errores nunca quedan en cachés (ASVS V8.2.1).
/// - `X-Robots-Tag: noindex` en la API.
pub async fn cabeceras_de_seguridad(peticion: Request, siguiente: Next) -> Response {
    let es_api = peticion.uri().path().starts_with("/api/");
    let mut respuesta = siguiente.run(peticion).await;
    let cabeceras = respuesta.headers_mut();
    cabeceras.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    cabeceras.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    cabeceras.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    cabeceras
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    if es_api {
        cabeceras.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        );
        cabeceras.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    }
    respuesta
}

/// `Strict-Transport-Security` de un año (ASVS V9.1.1, V14.4.5). Se aplica cuando el sitio se
/// sirve por HTTPS (`DIAMANTE_URL_PUBLICA` con `https://`): el navegador solo la toma de
/// respuestas HTTPS, así que detrás del proxy que termina TLS (Traefik) es segura. Sin
/// `includeSubDomains`: la API no sabe qué otros sitios cuelgan del mismo dominio.
pub async fn hsts(peticion: Request, siguiente: Next) -> Response {
    let mut respuesta = siguiente.run(peticion).await;
    respuesta.headers_mut().insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000"),
    );
    respuesta
}

/// Límites de uso por minuto (RNF-SEG, `docs/diseno/api.md` 1.4). Se suman a los propios de
/// inicio de sesión, recuperación y códigos de verificación, que son más estrictos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfiguracionLimites {
    /// `/api/v1/publico/*`, por IP.
    pub publico_por_ip: usize,
    /// Panel con sesión, por sesión.
    pub panel_por_sesion: usize,
    /// Panel con sesión, por IP (el equipo de la barbería comparte la IP del local). Impide
    /// saltarse el límite inventando cookies: cada una se valida contra la base.
    pub panel_por_ip: usize,
    /// Lo demás sin sesión (inicio de sesión, activación, rutas del panel sin cookie), por IP.
    pub anonimo_por_ip: usize,
}

impl Default for ConfiguracionLimites {
    fn default() -> Self {
        Self {
            publico_por_ip: 120,
            panel_por_sesion: 600,
            panel_por_ip: 1_200,
            anonimo_por_ip: 120,
        }
    }
}

#[derive(Debug)]
struct Limitadores {
    publico: LimitadorIntentos,
    sesion: LimitadorIntentos,
    ip_con_sesion: LimitadorIntentos,
    anonimo: LimitadorIntentos,
}

/// Estado de la capa [`limitar_uso`] (en memoria: la API corre en un solo proceso).
#[derive(Debug, Clone)]
pub struct LimitesDeUso(Arc<Limitadores>);

impl LimitesDeUso {
    pub fn nuevos(c: ConfiguracionLimites) -> Self {
        let minuto = Duration::from_secs(60);
        Self(Arc::new(Limitadores {
            publico: LimitadorIntentos::nuevo(c.publico_por_ip, minuto),
            sesion: LimitadorIntentos::nuevo(c.panel_por_sesion, minuto),
            ip_con_sesion: LimitadorIntentos::nuevo(c.panel_por_ip, minuto),
            anonimo: LimitadorIntentos::nuevo(c.anonimo_por_ip, minuto),
        }))
    }
}

impl Default for LimitesDeUso {
    fn default() -> Self {
        Self::nuevos(ConfiguracionLimites::default())
    }
}

/// Rutas sin límite: salud (Docker), webhooks (Meta reintenta y los firma), fotos públicas
/// (cacheadas por el navegador) y las de apoyo del ambiente de pruebas.
fn sin_limite(ruta: &str) -> bool {
    ruta.starts_with("/api/salud")
        || ruta.starts_with("/api/v1/webhooks/")
        || ruta.starts_with("/api/v1/_pruebas/")
        || ruta.starts_with("/media/")
}

/// Huella corta del token de sesión: el limitador no guarda tokens en memoria.
fn huella(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Aplica los límites de uso; al superarlos responde `429 demasiados_intentos` con
/// `Retry-After`. Debe ir por fuera del enrutador (ve la ruta completa) y por dentro de la
/// capa que agrega [`ConfianzaProxy`](super::ConfianzaProxy).
pub async fn limitar_uso(
    State(limites): State<LimitesDeUso>,
    IpCliente(ip): IpCliente,
    peticion: Request,
    siguiente: Next,
) -> Response {
    let ruta = peticion.uri().path();
    if sin_limite(ruta) {
        return siguiente.run(peticion).await;
    }
    let ip = ip.map_or_else(|| "desconocida".to_owned(), |ip| ip.to_string());
    let l = &limites.0;
    let resultado = if ruta.starts_with("/api/v1/publico/") {
        l.publico.intentar(&ip)
    } else if let Some(token) = leer_cookie(peticion.headers(), COOKIE_SESION) {
        l.ip_con_sesion
            .intentar(&ip)
            .and_then(|()| l.sesion.intentar(&huella(&token)))
    } else {
        l.anonimo.intentar(&ip)
    };
    match resultado {
        Ok(()) => siguiente.run(peticion).await,
        Err(espera) => ErrorHttp::demasiados_intentos(espera).into_response(),
    }
}

/// Plantilla de la ruta que atendió la petición, en la respuesta: la leen las capas de afuera
/// (las métricas), donde el enrutador todavía no ha elegido la ruta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RutaAtendida(pub String);

/// Anota en la traza de la petición (`ruta`) la plantilla de la ruta que atendió, p. ej.
/// `/api/v1/publico/citas/gestion/{token}`: los logs nunca llevan los tokens del camino ni la
/// consulta (`?q=` puede ser un nombre o un celular; `?firma=`, una firma). Se aplica con
/// `route_layer`, donde el enrutador ya eligió la ruta. También deja la plantilla en la
/// respuesta ([`RutaAtendida`]).
pub async fn anotar_ruta(peticion: Request, siguiente: Next) -> Response {
    let ruta = peticion
        .extensions()
        .get::<MatchedPath>()
        .map(|r| r.as_str().to_owned());
    if let Some(ruta) = &ruta {
        tracing::Span::current().record("ruta", tracing::field::display(ruta));
    }
    let mut respuesta = siguiente.run(peticion).await;
    if let Some(ruta) = ruta {
        respuesta.extensions_mut().insert(RutaAtendida(ruta));
    }
    respuesta
}

#[cfg(test)]
mod pruebas {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware::{from_fn, from_fn_with_state};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    fn app(limites: ConfiguracionLimites) -> Router {
        Router::new()
            .route("/api/v1/publico/landing", get(|| async { "landing" }))
            .route("/api/v1/clientes", get(|| async { "clientes" }))
            .route(
                "/api/v1/publico/servicios",
                get(|| async { ([(header::CACHE_CONTROL, "public, max-age=60")], "servicios") }),
            )
            .route("/media/publico/foto.jpg", get(|| async { "foto" }))
            .route("/api/salud", get(|| async { "viva" }))
            .layer(from_fn_with_state(LimitesDeUso::nuevos(limites), limitar_uso))
            .layer(from_fn(cabeceras_de_seguridad))
    }

    async fn pedir(app: &Router, ruta: &str, cookie: Option<&str>) -> Response {
        let mut peticion = Request::get(ruta);
        if let Some(token) = cookie {
            peticion = peticion.header(header::COOKIE, format!("{COOKIE_SESION}={token}"));
        }
        app.clone()
            .oneshot(peticion.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn la_api_lleva_las_cabeceras_de_seguridad() {
        let app = app(ConfiguracionLimites::default());
        let r = pedir(&app, "/api/v1/clientes", Some("t")).await;
        let h = r.headers();
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(h[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(
            h[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; frame-ancestors 'none'"
        );
        assert_eq!(h[header::CACHE_CONTROL], "no-store");
        assert_eq!(h["x-robots-tag"], "noindex");
        // El manejador que fija su caché la conserva.
        let r = pedir(&app, "/api/v1/publico/servicios", None).await;
        assert_eq!(r.headers()[header::CACHE_CONTROL], "public, max-age=60");
        // Las fotos no llevan CSP (se verían rotas al abrirlas directamente).
        let r = pedir(&app, "/media/publico/foto.jpg", None).await;
        assert!(!r.headers().contains_key(header::CONTENT_SECURITY_POLICY));
        assert_eq!(r.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    #[tokio::test]
    async fn limita_lo_publico_por_ip_y_el_panel_por_sesion() {
        let app = app(ConfiguracionLimites {
            publico_por_ip: 3,
            panel_por_sesion: 2,
            panel_por_ip: 5,
            anonimo_por_ip: 1,
        });
        for _ in 0..3 {
            assert_eq!(
                pedir(&app, "/api/v1/publico/landing", None).await.status(),
                StatusCode::OK
            );
        }
        let r = pedir(&app, "/api/v1/publico/landing", None).await;
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(r.headers().contains_key(header::RETRY_AFTER));
        assert_eq!(
            r.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff",
            "el 429 también"
        );
        // Cada sesión tiene su cuenta…
        for _ in 0..2 {
            assert_eq!(
                pedir(&app, "/api/v1/clientes", Some("a")).await.status(),
                StatusCode::OK
            );
        }
        assert_eq!(
            pedir(&app, "/api/v1/clientes", Some("a")).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            pedir(&app, "/api/v1/clientes", Some("b")).await.status(),
            StatusCode::OK
        );
        // …pero inventar cookies no salta el límite por IP (5: a, a, a(429 por sesión), b, c).
        assert_eq!(
            pedir(&app, "/api/v1/clientes", Some("c")).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            pedir(&app, "/api/v1/clientes", Some("d")).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        // Sin sesión fuera de lo público: su propio límite.
        assert_eq!(
            pedir(&app, "/api/v1/clientes", None).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            pedir(&app, "/api/v1/clientes", None).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        // La salud nunca se limita.
        for _ in 0..10 {
            assert_eq!(pedir(&app, "/api/salud", None).await.status(), StatusCode::OK);
        }
    }

    /// Salida de logs en memoria para revisar qué se escribe.
    #[derive(Clone, Default)]
    struct Salida(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Salida {
        fn write(&mut self, datos: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(datos);
            Ok(datos.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Salida {
        type Writer = Salida;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[tokio::test]
    async fn la_traza_lleva_la_plantilla_de_la_ruta_y_no_la_url() {
        use tracing::Instrument;

        let salida = Salida::default();
        let suscriptor = tracing_subscriber::fmt()
            .with_writer(salida.clone())
            .with_ansi(false)
            .finish();
        let _guarda = tracing::subscriber::set_default(suscriptor);
        let interno = Router::new()
            .route(
                "/publico/citas/gestion/{token}",
                get(|| async {
                    tracing::info!("atendida");
                    "ok"
                }),
            )
            .route_layer(from_fn(anotar_ruta));
        let app = Router::new().nest("/api/v1", interno);
        let span = tracing::info_span!("http", ruta = tracing::field::Empty);
        let peticion = Request::get("/api/v1/publico/citas/gestion/TokenSecreto123?q=3105559917")
            .body(Body::empty())
            .unwrap();
        let respuesta = app.oneshot(peticion).instrument(span).await.unwrap();
        assert_eq!(respuesta.status(), StatusCode::OK);
        let texto = String::from_utf8(salida.0.lock().unwrap().clone()).unwrap();
        assert!(
            texto.contains("ruta=/api/v1/publico/citas/gestion/{token}"),
            "{texto}"
        );
        assert!(
            !texto.contains("TokenSecreto123") && !texto.contains("3105559917"),
            "{texto}"
        );
    }

    #[test]
    fn la_huella_no_es_el_token() {
        let h = huella("token-secreto");
        assert_eq!(h.len(), 24);
        assert!(!h.contains("token"));
        assert_eq!(h, huella("token-secreto"));
    }
}
