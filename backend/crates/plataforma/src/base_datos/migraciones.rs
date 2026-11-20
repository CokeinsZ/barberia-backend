//! Migraciones por módulo.
//!
//! Cada módulo guarda sus migraciones en su carpeta (`modulos/<modulo>/migraciones/`) y las
//! publica con `sqlx::migrate!`. El comando `migrar` las reúne y las aplica en un solo orden
//! global, dado por la versión (marca de tiempo) de cada archivo.

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migration, Migrator};

/// Migraciones de un módulo, con el nombre del módulo para los mensajes de error.
#[derive(Debug, Clone, Copy)]
pub struct OrigenMigraciones {
    pub modulo: &'static str,
    pub migraciones: &'static Migrator,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorMigraciones {
    #[error("La versión {version} está repetida en los módulos «{primero}» y «{segundo}».")]
    VersionRepetida {
        version: i64,
        primero: &'static str,
        segundo: &'static str,
    },
    #[error("No se pudieron aplicar las migraciones: {0}")]
    Aplicacion(#[from] MigrateError),
}

/// Reúne las migraciones de todos los módulos en un solo migrador, ordenado por versión.
/// Falla si dos módulos usan la misma versión.
pub fn combinar_migraciones(origenes: &[OrigenMigraciones]) -> Result<Migrator, ErrorMigraciones> {
    let mut todas: Vec<(&'static str, Migration)> = origenes
        .iter()
        .flat_map(|o| o.migraciones.iter().map(move |m| (o.modulo, m.clone())))
        .collect();
    todas.sort_by_key(|(_, m)| m.version);
    for par in todas.windows(2) {
        if par[0].1.version == par[1].1.version {
            return Err(ErrorMigraciones::VersionRepetida {
                version: par[0].1.version,
                primero: par[0].0,
                segundo: par[1].0,
            });
        }
    }
    Ok(Migrator::with_migrations(
        todas.into_iter().map(|(_, m)| m).collect(),
    ))
}

/// Aplica las migraciones pendientes. Usa un bloqueo de PostgreSQL, así que es seguro aunque
/// varios procesos intenten migrar a la vez.
pub async fn aplicar_migraciones(pool: &PgPool, migrador: &Migrator) -> Result<(), ErrorMigraciones> {
    migrador.run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::base_datos::MIGRACIONES_BASE;

    #[test]
    fn detecta_versiones_repetidas() {
        let origenes = [
            OrigenMigraciones {
                modulo: "base",
                migraciones: &MIGRACIONES_BASE,
            },
            OrigenMigraciones {
                modulo: "copia",
                migraciones: &MIGRACIONES_BASE,
            },
        ];
        let error = combinar_migraciones(&origenes).unwrap_err();
        assert!(matches!(
            error,
            ErrorMigraciones::VersionRepetida {
                primero: "base",
                segundo: "copia",
                ..
            }
        ));
    }

    #[test]
    fn la_base_va_primero() {
        let migrador = combinar_migraciones(&[OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        }])
        .unwrap();
        assert_eq!(
            migrador.iter().next().map(|m| m.version),
            Some(20_261_201_000_000)
        );
    }
}
