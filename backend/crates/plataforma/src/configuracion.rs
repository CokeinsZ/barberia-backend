//! Configuración desde variables de entorno, validada al arrancar: si falta algo, el
//! proceso no inicia y el error dice exactamente qué falta.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entorno {
    Desarrollo,
    Pruebas,
    Produccion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoLogs {
    Texto,
    Json,
}

/// Texto secreto que nunca se imprime en logs ni en `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secreto(String);

impl Secreto {
    pub fn nuevo(valor: impl Into<String>) -> Self {
        Self(valor.into())
    }

    pub fn exponer(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secreto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secreto(***)")
    }
}

#[derive(Debug, Clone)]
pub struct ConfiguracionBaseDatos {
    /// Conexión de la aplicación (rol `diamante_app`).
    pub url: Secreto,
    /// Conexión para migraciones (rol dueño del esquema). Si no se define, se usa `url`.
    pub url_migraciones: Secreto,
    pub max_conexiones: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeguridadSmtp {
    /// Sin cifrado (solo para Mailpit en desarrollo).
    Ninguna,
    /// STARTTLS, normalmente en el puerto 587.
    StartTls,
    /// TLS implícito, normalmente en el puerto 465.
    Tls,
}

#[derive(Debug, Clone)]
pub struct ConfiguracionSmtp {
    pub host: String,
    pub puerto: u16,
    pub seguridad: SeguridadSmtp,
    pub usuario: Option<String>,
    pub clave: Option<Secreto>,
    /// Remitente, p. ej. `Diamante Barbería <no-responder@diamante.co>`.
    pub remitente: String,
}

#[derive(Debug, Clone)]
pub struct Configuracion {
    pub entorno: Entorno,
    pub direccion_http: SocketAddr,
    pub base_datos: ConfiguracionBaseDatos,
    pub formato_logs: FormatoLogs,
    /// URL pública del sitio, sin barra final. Se usa en los enlaces de los correos.
    pub url_publica: String,
    /// Orígenes aceptados en peticiones que modifican datos (verificación de `Origin`).
    pub origenes_permitidos: Vec<String>,
    /// Si la API está detrás de un proxy de confianza (Traefik), la IP del cliente se toma de
    /// `X-Forwarded-For`.
    pub confiar_proxy: bool,
    /// Envío de correos. Sin configuración, los correos quedan en cola sin enviarse.
    pub smtp: Option<ConfiguracionSmtp>,
    /// Carpeta raíz del almacenamiento de archivos (volumen `archivos`).
    pub archivos_dir: PathBuf,
    /// Clave para firmar las URL de los archivos privados (HMAC-SHA256, 32 caracteres o más).
    /// Obligatoria en producción; en desarrollo, si falta, se genera una al arrancar.
    pub clave_firma: Option<Secreto>,
    /// Clave AES-256 de los datos sensibles (32 bytes en base64). Obligatoria en producción;
    /// en desarrollo, si falta, se usa una clave fija de desarrollo.
    pub clave_cifrado: Option<ClaveCifrado>,
    /// Rutas de apoyo para pruebas (`/api/v1/_pruebas/…`): solo con `DIAMANTE_PRUEBAS_E2E=true`
    /// y nunca en producción.
    pub rutas_de_pruebas: bool,
    pub whatsapp: ConfiguracionWhatsApp,
    /// La API también ejecuta las tareas del worker (`DIAMANTE_WORKER_EN_API=true`). En el
    /// ambiente E2E así todo comparte un solo reloj (`/_pruebas/reloj`).
    pub worker_en_api: bool,
    /// Gotenberg (HTML → PDF), p. ej. `http://gotenberg:3000`, sin barra final. Sin él, los
    /// PDF de los reportes responden 503 y lo demás sigue funcionando.
    pub gotenberg_url: Option<String>,
    /// Límites de uso por IP y por sesión (`DIAMANTE_LIMITES_DE_USO`, activos por defecto). Solo
    /// las pruebas de carga los apagan; en producción no se pueden apagar.
    pub limites_de_uso: bool,
}

/// WhatsApp. Con las credenciales de Meta (`cloud_api`), los mensajes salen por WhatsApp Cloud
/// API; sin ellas, fuera de producción se usa el adaptador simulado y en producción los
/// mensajes quedan en cola hasta configurar Meta (como el correo sin SMTP).
#[derive(Debug, Clone)]
pub struct ConfiguracionWhatsApp {
    pub simulado: bool,
    pub cloud_api: Option<CloudApiWhatsApp>,
    /// Webhook de Meta (`/api/v1/webhooks/whatsapp`). Sin él, el webhook rechaza todo.
    pub webhook: Option<WebhookWhatsApp>,
}

/// Envío por WhatsApp Cloud API (Meta).
#[derive(Debug, Clone)]
pub struct CloudApiWhatsApp {
    /// Token de acceso permanente de un usuario del sistema (`DIAMANTE_WHATSAPP_TOKEN`).
    pub token: Secreto,
    /// Identificador del número de teléfono en Meta (`DIAMANTE_WHATSAPP_ID_NUMERO`).
    pub id_numero: String,
    /// Idioma con que se registraron las plantillas (`DIAMANTE_WHATSAPP_IDIOMA`, `es_CO`).
    pub idioma: String,
    /// Base de la Graph API con su versión, sin barra final (`DIAMANTE_WHATSAPP_API`).
    pub api: String,
}

/// Idioma de las plantillas por defecto: español (Colombia).
pub const IDIOMA_WHATSAPP: &str = "es_CO";
/// Graph API por defecto (v26.0, julio de 2026; cada versión dura unos dos años).
pub const API_WHATSAPP: &str = "https://graph.facebook.com/v26.0";

#[derive(Debug, Clone)]
pub struct WebhookWhatsApp {
    /// Secreto de la app de Meta: firma `X-Hub-Signature-256`.
    pub secreto_app: Secreto,
    /// Token que Meta envía al registrar el webhook (`hub.verify_token`).
    pub token_verificacion: Secreto,
}

/// Clave AES-256 que nunca se imprime en logs ni en `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaveCifrado(pub [u8; 32]);

impl fmt::Debug for ClaveCifrado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClaveCifrado(***)")
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Configuración inválida. Faltan: [{}]. Inválidas: [{}].", faltantes.join(", "), invalidas.join("; "))]
pub struct ErrorConfiguracion {
    pub faltantes: Vec<&'static str>,
    pub invalidas: Vec<String>,
}

impl Configuracion {
    /// Lee la configuración de las variables de entorno del proceso.
    pub fn desde_entorno() -> Result<Self, ErrorConfiguracion> {
        Self::desde(|clave| std::env::var(clave).ok().filter(|v| !v.trim().is_empty()))
    }

    /// Solo lo que necesita `diamante migrar`: la conexión del rol dueño del esquema
    /// (`MIGRACIONES_DATABASE_URL` o `DATABASE_URL`) y el formato de los logs. Las migraciones no
    /// usan la URL pública ni las claves, así que su contenedor no las recibe (ni las exige,
    /// aunque sea producción).
    pub fn para_migraciones(
        leer: impl Fn(&str) -> Option<String>,
    ) -> Result<(Secreto, FormatoLogs), ErrorConfiguracion> {
        let url = leer("MIGRACIONES_DATABASE_URL").or_else(|| leer("DATABASE_URL"));
        if let Some(error) = url.as_deref().and_then(|u| url_invalida("DATABASE_URL", u)) {
            return Err(ErrorConfiguracion {
                faltantes: Vec::new(),
                invalidas: vec![error],
            });
        }
        let produccion = leer("DIAMANTE_ENTORNO").as_deref() == Some("produccion");
        let formato = match leer("DIAMANTE_LOGS").as_deref() {
            Some("json") => Some(FormatoLogs::Json),
            Some("texto") => Some(FormatoLogs::Texto),
            None if produccion => Some(FormatoLogs::Json),
            None => Some(FormatoLogs::Texto),
            Some(_) => None,
        };
        match (url, formato) {
            (Some(url), Some(formato)) => Ok((Secreto::nuevo(url), formato)),
            (url, formato) => Err(ErrorConfiguracion {
                faltantes: if url.is_none() {
                    vec!["DATABASE_URL"]
                } else {
                    Vec::new()
                },
                invalidas: if formato.is_none() {
                    vec!["DIAMANTE_LOGS (use texto o json)".to_owned()]
                } else {
                    Vec::new()
                },
            }),
        }
    }

    /// Lee la configuración de cualquier fuente (permite probarla sin tocar el entorno).
    pub fn desde(leer: impl Fn(&str) -> Option<String>) -> Result<Self, ErrorConfiguracion> {
        let mut faltantes = Vec::new();
        let mut invalidas = Vec::new();

        let entorno = match leer("DIAMANTE_ENTORNO").as_deref() {
            None | Some("desarrollo") => Entorno::Desarrollo,
            Some("pruebas") => Entorno::Pruebas,
            Some("produccion") => Entorno::Produccion,
            Some(otro) => {
                invalidas.push(format!(
                    "DIAMANTE_ENTORNO={otro} (use desarrollo, pruebas o produccion)"
                ));
                Entorno::Desarrollo
            }
        };

        let direccion_http = leer("DIAMANTE_HTTP_DIRECCION").unwrap_or_else(|| "0.0.0.0:8080".into());
        let direccion_http = direccion_http.parse().unwrap_or_else(|_| {
            invalidas.push(format!(
                "DIAMANTE_HTTP_DIRECCION={direccion_http} (ejemplo: 0.0.0.0:8080)"
            ));
            SocketAddr::from(([0, 0, 0, 0], 8080))
        });

        let url = leer("DATABASE_URL");
        if url.is_none() {
            faltantes.push("DATABASE_URL");
        }
        let url = url.unwrap_or_default();
        if !url.is_empty()
            && let Some(error) = url_invalida("DATABASE_URL", &url)
        {
            invalidas.push(error);
        }
        let url_migraciones = leer("MIGRACIONES_DATABASE_URL").unwrap_or_else(|| url.clone());

        let max_conexiones = match leer("DIAMANTE_DB_MAX_CONEXIONES") {
            None => 10,
            Some(texto) => match texto.parse::<u32>() {
                Ok(n) if (1..=100).contains(&n) => n,
                _ => {
                    invalidas.push(format!("DIAMANTE_DB_MAX_CONEXIONES={texto} (entre 1 y 100)"));
                    10
                }
            },
        };

        let formato_logs = match leer("DIAMANTE_LOGS").as_deref() {
            Some("json") => FormatoLogs::Json,
            Some("texto") => FormatoLogs::Texto,
            None if entorno == Entorno::Produccion => FormatoLogs::Json,
            None => FormatoLogs::Texto,
            Some(otro) => {
                invalidas.push(format!("DIAMANTE_LOGS={otro} (use texto o json)"));
                FormatoLogs::Texto
            }
        };

        let url_publica = match leer("DIAMANTE_URL_PUBLICA") {
            Some(url) => url.trim_end_matches('/').to_owned(),
            None if entorno == Entorno::Produccion => {
                faltantes.push("DIAMANTE_URL_PUBLICA");
                String::new()
            }
            None => "http://localhost:8080".to_owned(),
        };
        if !url_publica.is_empty()
            && !(url_publica.starts_with("http://") || url_publica.starts_with("https://"))
        {
            invalidas.push(format!(
                "DIAMANTE_URL_PUBLICA={url_publica} (debe empezar por http:// o https://)"
            ));
        }
        let mut origenes_permitidos = vec![origen_de(&url_publica)];
        if let Some(extra) = leer("DIAMANTE_ORIGENES") {
            origenes_permitidos.extend(
                extra
                    .split(',')
                    .map(|o| o.trim().trim_end_matches('/').to_owned())
                    .filter(|o| !o.is_empty()),
            );
        }

        let confiar_proxy = match leer("DIAMANTE_CONFIAR_PROXY").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            Some(otro) => {
                invalidas.push(format!("DIAMANTE_CONFIAR_PROXY={otro} (use true o false)"));
                false
            }
        };

        let smtp = leer("DIAMANTE_SMTP_HOST").map(|host| {
            let seguridad = match leer("DIAMANTE_SMTP_SEGURIDAD").as_deref() {
                None | Some("starttls") => SeguridadSmtp::StartTls,
                Some("tls") => SeguridadSmtp::Tls,
                Some("ninguna") => SeguridadSmtp::Ninguna,
                Some(otro) => {
                    invalidas.push(format!(
                        "DIAMANTE_SMTP_SEGURIDAD={otro} (use starttls, tls o ninguna)"
                    ));
                    SeguridadSmtp::StartTls
                }
            };
            let puerto_defecto = match seguridad {
                SeguridadSmtp::Ninguna => 25,
                SeguridadSmtp::StartTls => 587,
                SeguridadSmtp::Tls => 465,
            };
            let puerto = match leer("DIAMANTE_SMTP_PUERTO") {
                None => puerto_defecto,
                Some(texto) => texto.parse().unwrap_or_else(|_| {
                    invalidas.push(format!("DIAMANTE_SMTP_PUERTO={texto}"));
                    puerto_defecto
                }),
            };
            // Las dos combinaciones cruzadas de los puertos estándar dejan el envío colgado.
            match (puerto, seguridad) {
                (465, SeguridadSmtp::StartTls) => invalidas.push(
                    "DIAMANTE_SMTP_PUERTO=465 usa TLS directo: ponga DIAMANTE_SMTP_SEGURIDAD=tls \
                     (o el puerto 587 con starttls)"
                        .to_owned(),
                ),
                (587, SeguridadSmtp::Tls) => invalidas.push(
                    "DIAMANTE_SMTP_PUERTO=587 usa STARTTLS: ponga DIAMANTE_SMTP_SEGURIDAD=starttls \
                     (o el puerto 465 con tls)"
                        .to_owned(),
                ),
                _ => {}
            }
            let remitente = leer("DIAMANTE_SMTP_REMITENTE").unwrap_or_else(|| {
                faltantes.push("DIAMANTE_SMTP_REMITENTE");
                String::new()
            });
            let usuario = leer("DIAMANTE_SMTP_USUARIO");
            let clave = leer("DIAMANTE_SMTP_CLAVE").map(Secreto::nuevo);
            if usuario.is_some() != clave.is_some() {
                invalidas.push("DIAMANTE_SMTP_USUARIO y DIAMANTE_SMTP_CLAVE van juntos".to_owned());
            }
            ConfiguracionSmtp {
                host,
                puerto,
                seguridad,
                usuario,
                clave,
                remitente,
            }
        });

        let archivos_dir =
            PathBuf::from(leer("DIAMANTE_ARCHIVOS_DIR").unwrap_or_else(|| "/datos/archivos".into()));

        let clave_firma = leer("DIAMANTE_CLAVE_FIRMA").map(Secreto::nuevo);
        match &clave_firma {
            Some(clave) if clave.exponer().chars().count() < 32 => {
                invalidas.push("DIAMANTE_CLAVE_FIRMA (use 32 caracteres o más)".to_owned());
            }
            None if entorno == Entorno::Produccion => faltantes.push("DIAMANTE_CLAVE_FIRMA"),
            _ => {}
        }

        let clave_cifrado = match leer("DIAMANTE_CLAVE_CIFRADO") {
            Some(texto) => match decodificar_clave(&texto) {
                Some(clave) => Some(ClaveCifrado(clave)),
                None => {
                    invalidas.push(
                        "DIAMANTE_CLAVE_CIFRADO (32 bytes en base64; genere una con `openssl rand -base64 32`)"
                            .to_owned(),
                    );
                    None
                }
            },
            None => {
                if entorno == Entorno::Produccion {
                    faltantes.push("DIAMANTE_CLAVE_CIFRADO");
                }
                None
            }
        };

        let rutas_de_pruebas = match leer("DIAMANTE_PRUEBAS_E2E").as_deref() {
            None | Some("false") => false,
            Some("true") if entorno == Entorno::Produccion => {
                invalidas.push("DIAMANTE_PRUEBAS_E2E=true no se permite en producción".to_owned());
                false
            }
            Some("true") => true,
            Some(otro) => {
                invalidas.push(format!("DIAMANTE_PRUEBAS_E2E={otro} (use true o false)"));
                false
            }
        };

        let cloud_api = match (
            leer("DIAMANTE_WHATSAPP_TOKEN"),
            leer("DIAMANTE_WHATSAPP_ID_NUMERO"),
        ) {
            (Some(token), Some(id_numero)) => {
                let id_numero = id_numero.trim().to_owned();
                if id_numero.is_empty() || !id_numero.bytes().all(|b| b.is_ascii_digit()) {
                    invalidas.push(
                        "DIAMANTE_WHATSAPP_ID_NUMERO (el identificador numérico del número en Meta)"
                            .to_owned(),
                    );
                }
                let idioma = leer("DIAMANTE_WHATSAPP_IDIOMA").unwrap_or_else(|| IDIOMA_WHATSAPP.to_owned());
                let valido = idioma.len() >= 2
                    && idioma.len() <= 6
                    && idioma.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_');
                if !valido {
                    invalidas.push(format!("DIAMANTE_WHATSAPP_IDIOMA={idioma} (p. ej., es_CO)"));
                }
                let api = leer("DIAMANTE_WHATSAPP_API")
                    .unwrap_or_else(|| API_WHATSAPP.to_owned())
                    .trim_end_matches('/')
                    .to_owned();
                if !api.starts_with("https://")
                    && !(entorno != Entorno::Produccion && api.starts_with("http://"))
                {
                    invalidas.push("DIAMANTE_WHATSAPP_API (debe ser https://…)".to_owned());
                }
                Some(CloudApiWhatsApp {
                    token: Secreto::nuevo(token.trim().to_owned()),
                    id_numero,
                    idioma,
                    api,
                })
            }
            (None, None) => None,
            _ => {
                invalidas.push("DIAMANTE_WHATSAPP_TOKEN y DIAMANTE_WHATSAPP_ID_NUMERO van juntas".to_owned());
                None
            }
        };

        let webhook = match (
            leer("DIAMANTE_WHATSAPP_SECRETO_APP"),
            leer("DIAMANTE_WHATSAPP_TOKEN_VERIFICACION"),
        ) {
            (Some(secreto), Some(token)) => Some(WebhookWhatsApp {
                secreto_app: Secreto::nuevo(secreto),
                token_verificacion: Secreto::nuevo(token),
            }),
            (None, None) => None,
            _ => {
                invalidas.push(
                    "DIAMANTE_WHATSAPP_SECRETO_APP y DIAMANTE_WHATSAPP_TOKEN_VERIFICACION van juntas"
                        .to_owned(),
                );
                None
            }
        };
        let worker_en_api = match leer("DIAMANTE_WORKER_EN_API").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            Some(otro) => {
                invalidas.push(format!("DIAMANTE_WORKER_EN_API={otro} (use true o false)"));
                false
            }
        };

        let limites_de_uso = match leer("DIAMANTE_LIMITES_DE_USO").as_deref() {
            None | Some("true") => true,
            Some("false") if entorno == Entorno::Produccion => {
                invalidas.push("DIAMANTE_LIMITES_DE_USO=false no se permite en producción".to_owned());
                true
            }
            Some("false") => false,
            Some(otro) => {
                invalidas.push(format!("DIAMANTE_LIMITES_DE_USO={otro} (use true o false)"));
                true
            }
        };
        let gotenberg_url = leer("DIAMANTE_GOTENBERG_URL").map(|url| url.trim_end_matches('/').to_owned());
        if let Some(url) = &gotenberg_url
            && !(url.starts_with("http://") || url.starts_with("https://"))
        {
            invalidas.push(format!(
                "DIAMANTE_GOTENBERG_URL={url} (debe empezar por http:// o https://)"
            ));
        }

        if !faltantes.is_empty() || !invalidas.is_empty() {
            return Err(ErrorConfiguracion { faltantes, invalidas });
        }
        Ok(Self {
            entorno,
            direccion_http,
            base_datos: ConfiguracionBaseDatos {
                url: Secreto::nuevo(url),
                url_migraciones: Secreto::nuevo(url_migraciones),
                max_conexiones,
            },
            formato_logs,
            url_publica,
            origenes_permitidos,
            confiar_proxy,
            smtp,
            archivos_dir,
            clave_firma,
            clave_cifrado,
            rutas_de_pruebas,
            whatsapp: ConfiguracionWhatsApp {
                simulado: entorno != Entorno::Produccion && cloud_api.is_none(),
                cloud_api,
                webhook,
            },
            worker_en_api,
            gotenberg_url,
            limites_de_uso,
        })
    }
}

/// Explica por qué no sirve una URL de PostgreSQL, sin mostrarla (puede llevar la contraseña).
fn url_invalida(variable: &str, url: &str) -> Option<String> {
    use std::str::FromStr;
    sqlx::postgres::PgConnectOptions::from_str(url)
        .err()
        .map(|error| {
            format!(
                "{variable} no es una URL de PostgreSQL válida ({error}). Si la contraseña tiene / @ : ? # \
             o %, no la ponga en la URL: use PGPASSWORD (así lo hace docker-compose.yml)"
            )
        })
}

/// Clave de 32 bytes en base64 (estándar o URL, con o sin relleno).
fn decodificar_clave(texto: &str) -> Option<[u8; 32]> {
    let limpio = texto.trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(limpio)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(limpio.trim_end_matches('=')))
        .ok()?;
    bytes.try_into().ok()
}

/// `https://diamante.co/ruta` → `https://diamante.co` (esquema, host y puerto).
fn origen_de(url: &str) -> String {
    match url.split_once("://") {
        Some((esquema, resto)) => format!("{esquema}://{}", resto.split('/').next().unwrap_or_default()),
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod pruebas {
    use std::collections::HashMap;

    use super::*;

    fn fuente(pares: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mapa: HashMap<String, String> = pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |clave| mapa.get(clave).cloned()
    }

    #[test]
    fn usa_valores_por_defecto_razonables() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert_eq!(c.entorno, Entorno::Desarrollo);
        assert_eq!(c.direccion_http.port(), 8080);
        assert_eq!(c.base_datos.max_conexiones, 10);
        assert_eq!(
            c.base_datos.url_migraciones.exponer(),
            "postgres://app@db/diamante"
        );
        assert_eq!(c.formato_logs, FormatoLogs::Texto);
    }

    /// Claves válidas de producción para las pruebas.
    const FIRMA: (&str, &str) = (
        "DIAMANTE_CLAVE_FIRMA",
        "una-clave-de-firma-de-al-menos-32-caracteres",
    );
    const CIFRADO: (&str, &str) = (
        "DIAMANTE_CLAVE_CIFRADO",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
    );

    #[test]
    fn produccion_registra_en_json() {
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co/"),
            FIRMA,
            CIFRADO,
        ]))
        .unwrap();
        assert_eq!(c.formato_logs, FormatoLogs::Json);
        assert_eq!(c.url_publica, "https://diamante.co");
    }

    #[test]
    fn produccion_exige_la_url_publica() {
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            FIRMA,
            CIFRADO,
        ]))
        .unwrap_err();
        assert_eq!(error.faltantes, ["DIAMANTE_URL_PUBLICA"]);
    }

    #[test]
    fn produccion_exige_las_claves_y_las_valida() {
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
        ]))
        .unwrap_err();
        assert_eq!(
            error.faltantes,
            ["DIAMANTE_CLAVE_FIRMA", "DIAMANTE_CLAVE_CIFRADO"]
        );

        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_CLAVE_FIRMA", "corta"),
            ("DIAMANTE_CLAVE_CIFRADO", "no-es-base64-de-32-bytes"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 2);

        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            FIRMA,
            CIFRADO,
        ]))
        .unwrap();
        assert_eq!(c.clave_cifrado.as_ref().map(|c| c.0[31]), Some(31));
        assert!(!format!("{c:?}").contains("una-clave-de-firma"));
    }

    #[test]
    fn las_rutas_de_pruebas_nunca_se_activan_en_produccion() {
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_PRUEBAS_E2E", "true"),
        ]))
        .unwrap();
        assert!(c.rutas_de_pruebas);
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
            FIRMA,
            CIFRADO,
            ("DIAMANTE_PRUEBAS_E2E", "true"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 1);
    }

    #[test]
    fn los_limites_de_uso_solo_se_apagan_fuera_de_produccion() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert!(c.limites_de_uso);
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_LIMITES_DE_USO", "false"),
        ]))
        .unwrap();
        assert!(!c.limites_de_uso);
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
            FIRMA,
            CIFRADO,
            ("DIAMANTE_LIMITES_DE_USO", "false"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 1);
    }

    #[test]
    fn una_contrasena_con_barras_en_la_url_da_un_error_claro_sin_mostrarla() {
        let url = "postgres://diamante_app:xXlv/7fF+C=@db:5432/diamante";
        let error = Configuracion::desde(fuente(&[("DATABASE_URL", url)])).unwrap_err();
        assert_eq!(error.invalidas.len(), 1, "{error:?}");
        assert!(error.invalidas[0].contains("PGPASSWORD") && !error.invalidas[0].contains("xXlv"));
        let error = Configuracion::para_migraciones(fuente(&[("DATABASE_URL", url)])).unwrap_err();
        assert!(error.invalidas[0].contains("PGPASSWORD"));
        // Sin contraseña en la URL (va en PGPASSWORD), cualquier carácter sirve.
        assert!(
            Configuracion::desde(fuente(&[(
                "DATABASE_URL",
                "postgres://diamante_app@db:5432/diamante"
            )]))
            .is_ok()
        );
    }

    #[test]
    fn smtp_avisa_si_el_puerto_no_corresponde_a_la_seguridad() {
        let con = |puerto: &str, seguridad: &str| {
            Configuracion::desde(fuente(&[
                ("DATABASE_URL", "postgres://app@db/diamante"),
                ("DIAMANTE_SMTP_HOST", "smtp.gmail.com"),
                ("DIAMANTE_SMTP_PUERTO", puerto),
                ("DIAMANTE_SMTP_SEGURIDAD", seguridad),
                ("DIAMANTE_SMTP_REMITENTE", "a@b.co"),
            ]))
        };
        assert!(con("465", "starttls").unwrap_err().invalidas[0].contains("tls"));
        assert!(con("587", "tls").is_err());
        assert!(con("465", "tls").is_ok() && con("587", "starttls").is_ok());
    }

    #[test]
    fn las_migraciones_solo_necesitan_la_base_aunque_sea_produccion() {
        let (url, formato) = Configuracion::para_migraciones(fuente(&[
            ("DATABASE_URL", "postgres://dueno@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
        ]))
        .unwrap();
        assert_eq!(
            (url.exponer(), formato),
            ("postgres://dueno@db/diamante", FormatoLogs::Json)
        );
        let (url, _) = Configuracion::para_migraciones(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("MIGRACIONES_DATABASE_URL", "postgres://dueno@db/diamante"),
        ]))
        .unwrap();
        assert_eq!(url.exponer(), "postgres://dueno@db/diamante");
        let error =
            Configuracion::para_migraciones(fuente(&[("DIAMANTE_ENTORNO", "produccion")])).unwrap_err();
        assert_eq!(error.faltantes, vec!["DATABASE_URL"]);
    }

    #[test]
    fn gotenberg_es_opcional_y_debe_ser_una_url() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert!(c.gotenberg_url.is_none());
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_GOTENBERG_URL", "http://gotenberg:3000/"),
        ]))
        .unwrap();
        assert_eq!(c.gotenberg_url.as_deref(), Some("http://gotenberg:3000"));
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_GOTENBERG_URL", "gotenberg:3000"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 1);
    }

    #[test]
    fn whatsapp_simulado_fuera_de_produccion_y_secretos_del_webhook_juntos() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert!(c.whatsapp.simulado && c.whatsapp.webhook.is_none() && !c.worker_en_api);
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_WHATSAPP_SECRETO_APP", "s"),
            ("DIAMANTE_WHATSAPP_TOKEN_VERIFICACION", "t"),
            ("DIAMANTE_WORKER_EN_API", "true"),
        ]))
        .unwrap();
        assert!(c.whatsapp.webhook.is_some() && c.worker_en_api);
        assert!(!format!("{c:?}").contains("\"s\""));
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_WHATSAPP_SECRETO_APP", "s"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 1);
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
            FIRMA,
            CIFRADO,
        ]))
        .unwrap();
        assert!(!c.whatsapp.simulado, "en producción nunca se simula");
    }

    #[test]
    fn whatsapp_cloud_api_con_token_e_id_del_numero() {
        let base = [
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
            FIRMA,
            CIFRADO,
        ];
        let con = |extra: &[(&'static str, &'static str)]| {
            let mut variables = base.to_vec();
            variables.extend_from_slice(extra);
            Configuracion::desde(fuente(&variables))
        };
        let c = con(&[
            ("DIAMANTE_WHATSAPP_TOKEN", "EAAG-token-secreto"),
            ("DIAMANTE_WHATSAPP_ID_NUMERO", "123456789012345"),
        ])
        .unwrap();
        let cloud = c.whatsapp.cloud_api.as_ref().unwrap();
        assert_eq!(
            (
                cloud.id_numero.as_str(),
                cloud.idioma.as_str(),
                cloud.api.as_str()
            ),
            ("123456789012345", "es_CO", "https://graph.facebook.com/v26.0")
        );
        assert!(!c.whatsapp.simulado);
        assert!(
            !format!("{c:?}").contains("EAAG-token-secreto"),
            "el token nunca se imprime"
        );

        let c = con(&[
            ("DIAMANTE_WHATSAPP_TOKEN", "t"),
            ("DIAMANTE_WHATSAPP_ID_NUMERO", "1"),
            ("DIAMANTE_WHATSAPP_IDIOMA", "es"),
            ("DIAMANTE_WHATSAPP_API", "https://graph.facebook.com/v27.0/"),
        ])
        .unwrap();
        let cloud = c.whatsapp.cloud_api.unwrap();
        assert_eq!(
            (cloud.idioma.as_str(), cloud.api.as_str()),
            ("es", "https://graph.facebook.com/v27.0")
        );

        for (extra, invalidas) in [
            (&[("DIAMANTE_WHATSAPP_TOKEN", "t")][..], 1),
            (
                &[
                    ("DIAMANTE_WHATSAPP_TOKEN", "t"),
                    ("DIAMANTE_WHATSAPP_ID_NUMERO", "no-es-numero"),
                ][..],
                1,
            ),
            (
                &[
                    ("DIAMANTE_WHATSAPP_TOKEN", "t"),
                    ("DIAMANTE_WHATSAPP_ID_NUMERO", "1"),
                    ("DIAMANTE_WHATSAPP_IDIOMA", "español"),
                    ("DIAMANTE_WHATSAPP_API", "http://graph.facebook.com"),
                ][..],
                2,
            ),
        ] {
            assert_eq!(con(extra).unwrap_err().invalidas.len(), invalidas, "{extra:?}");
        }
        // Fuera de producción, la API puede ser http (un Meta de mentira en las pruebas) y con
        // credenciales no se simula.
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_WHATSAPP_TOKEN", "t"),
            ("DIAMANTE_WHATSAPP_ID_NUMERO", "1"),
            ("DIAMANTE_WHATSAPP_API", "http://127.0.0.1:9999/v26.0"),
        ]))
        .unwrap();
        assert!(!c.whatsapp.simulado && c.whatsapp.cloud_api.is_some());
    }

    #[test]
    fn en_desarrollo_las_claves_y_la_carpeta_tienen_valores_por_defecto() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert!(c.clave_firma.is_none() && c.clave_cifrado.is_none());
        assert_eq!(c.archivos_dir, PathBuf::from("/datos/archivos"));
    }

    #[test]
    fn deriva_los_origenes_permitidos() {
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co/sitio"),
            ("DIAMANTE_ORIGENES", " http://localhost:4321/ ,"),
        ]))
        .unwrap();
        assert_eq!(
            c.origenes_permitidos,
            ["https://diamante.co", "http://localhost:4321"]
        );
    }

    #[test]
    fn configura_smtp_solo_si_hay_host() {
        let sin = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app@db/diamante")])).unwrap();
        assert!(sin.smtp.is_none());
        let con = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_SMTP_HOST", "mailpit"),
            ("DIAMANTE_SMTP_SEGURIDAD", "ninguna"),
            ("DIAMANTE_SMTP_REMITENTE", "Diamante <no-responder@diamante.co>"),
        ]))
        .unwrap();
        let smtp = con.smtp.unwrap();
        assert_eq!((smtp.puerto, smtp.seguridad), (25, SeguridadSmtp::Ninguna));
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "postgres://app@db/diamante"),
            ("DIAMANTE_SMTP_HOST", "smtp.x.co"),
        ]))
        .unwrap_err();
        assert_eq!(error.faltantes, ["DIAMANTE_SMTP_REMITENTE"]);
    }

    #[test]
    fn informa_todo_lo_que_falta_o_esta_mal() {
        let error = Configuracion::desde(fuente(&[
            ("DIAMANTE_ENTORNO", "staging"),
            ("DIAMANTE_DB_MAX_CONEXIONES", "0"),
        ]))
        .unwrap_err();
        assert_eq!(error.faltantes, ["DATABASE_URL"]);
        assert_eq!(error.invalidas.len(), 2);
    }

    #[test]
    fn no_imprime_secretos() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app:secreto-xyz@db/x")])).unwrap();
        assert!(!format!("{c:?}").contains("secreto-xyz"));
    }
}
