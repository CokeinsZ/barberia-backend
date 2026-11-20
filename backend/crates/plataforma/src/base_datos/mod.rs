//! Conexión a PostgreSQL, unidad de trabajo, migraciones combinadas y traducción de errores.

mod migraciones;
#[cfg(feature = "pruebas")]
pub mod pruebas;
mod transacciones;
mod unidad_de_trabajo;

use std::str::FromStr;
use std::time::Duration;

use nucleo::ErrorPersistencia;
use sqlx::PgPool;
use sqlx::error::ErrorKind;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

pub use migraciones::{ErrorMigraciones, OrigenMigraciones, aplicar_migraciones, combinar_migraciones};
pub use transacciones::TransaccionesPg;
pub use unidad_de_trabajo::UnidadDeTrabajo;

use crate::configuracion::Secreto;

/// Migraciones de la base compartida: extensiones y esquema `nucleo`. Van antes que las de los módulos.
pub static MIGRACIONES_BASE: Migrator = sqlx::migrate!("./migraciones");

/// Crea el pool de conexiones. Cada conexión trabaja en UTC: la conversión a la hora de Bogotá
/// la hace la aplicación (RNF-INT-04).
pub async fn conectar(url: &Secreto, max_conexiones: u32) -> Result<PgPool, sqlx::Error> {
    let opciones = PgConnectOptions::from_str(url.exponer())?
        .application_name("diamante")
        .options([("timezone", "UTC")]);
    PgPoolOptions::new()
        .max_connections(max_conexiones)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opciones)
        .await
}

/// Traduce un error de SQLx al error de persistencia del núcleo, conservando el nombre de la
/// restricción violada para que el caso de uso decida el mensaje (p. ej. `cita_sin_solape`).
pub fn a_error_persistencia(error: sqlx::Error) -> ErrorPersistencia {
    if let sqlx::Error::Database(ref db) = error {
        let restriccion = db.constraint().unwrap_or_default().to_owned();
        let es_exclusion = db.code().as_deref() == Some("23P01");
        match db.kind() {
            ErrorKind::UniqueViolation => return ErrorPersistencia::Duplicado { restriccion },
            ErrorKind::CheckViolation | ErrorKind::ForeignKeyViolation | ErrorKind::NotNullViolation => {
                return ErrorPersistencia::Restriccion { restriccion };
            }
            _ if es_exclusion => return ErrorPersistencia::Conflicto { restriccion },
            _ => {}
        }
    }
    ErrorPersistencia::Otro(error.to_string())
}
