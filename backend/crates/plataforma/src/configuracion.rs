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
    /// Si la API está detrás de un proxy de confianza (Caddy), la IP del cliente se toma de
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
        })
    }
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
            ("DATABASE_URL", "x"),
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
            ("DATABASE_URL", "x"),
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
            ("DATABASE_URL", "x"),
            ("DIAMANTE_ENTORNO", "produccion"),
            ("DIAMANTE_URL_PUBLICA", "https://diamante.co"),
        ]))
        .unwrap_err();
        assert_eq!(
            error.faltantes,
            ["DIAMANTE_CLAVE_FIRMA", "DIAMANTE_CLAVE_CIFRADO"]
        );

        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
            ("DIAMANTE_CLAVE_FIRMA", "corta"),
            ("DIAMANTE_CLAVE_CIFRADO", "no-es-base64-de-32-bytes"),
        ]))
        .unwrap_err();
        assert_eq!(error.invalidas.len(), 2);

        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "x"), FIRMA, CIFRADO])).unwrap();
        assert_eq!(c.clave_cifrado.as_ref().map(|c| c.0[31]), Some(31));
        assert!(!format!("{c:?}").contains("una-clave-de-firma"));
    }

    #[test]
    fn las_rutas_de_pruebas_nunca_se_activan_en_produccion() {
        let c =
            Configuracion::desde(fuente(&[("DATABASE_URL", "x"), ("DIAMANTE_PRUEBAS_E2E", "true")])).unwrap();
        assert!(c.rutas_de_pruebas);
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
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
    fn en_desarrollo_las_claves_y_la_carpeta_tienen_valores_por_defecto() {
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "x")])).unwrap();
        assert!(c.clave_firma.is_none() && c.clave_cifrado.is_none());
        assert_eq!(c.archivos_dir, PathBuf::from("/datos/archivos"));
    }

    #[test]
    fn deriva_los_origenes_permitidos() {
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
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
        let sin = Configuracion::desde(fuente(&[("DATABASE_URL", "x")])).unwrap();
        assert!(sin.smtp.is_none());
        let con = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
            ("DIAMANTE_SMTP_HOST", "mailpit"),
            ("DIAMANTE_SMTP_SEGURIDAD", "ninguna"),
            ("DIAMANTE_SMTP_REMITENTE", "Diamante <no-responder@diamante.co>"),
        ]))
        .unwrap();
        let smtp = con.smtp.unwrap();
        assert_eq!((smtp.puerto, smtp.seguridad), (25, SeguridadSmtp::Ninguna));
        let error = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
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
