//! Configuración desde variables de entorno, validada al arrancar: si falta algo, el
//! proceso no inicia y el error dice exactamente qué falta.

use std::fmt;
use std::net::SocketAddr;

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

#[derive(Debug, Clone)]
pub struct Configuracion {
    pub entorno: Entorno,
    pub direccion_http: SocketAddr,
    pub base_datos: ConfiguracionBaseDatos,
    pub formato_logs: FormatoLogs,
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
        })
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

    #[test]
    fn produccion_registra_en_json() {
        let c = Configuracion::desde(fuente(&[
            ("DATABASE_URL", "x"),
            ("DIAMANTE_ENTORNO", "produccion"),
        ]))
        .unwrap();
        assert_eq!(c.formato_logs, FormatoLogs::Json);
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
        let c = Configuracion::desde(fuente(&[("DATABASE_URL", "postgres://app:clave@db/x")])).unwrap();
        assert!(!format!("{c:?}").contains("clave"));
    }
}
