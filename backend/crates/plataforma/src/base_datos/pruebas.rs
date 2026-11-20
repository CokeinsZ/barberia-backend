//! Base de datos efímera para pruebas de integración (feature `pruebas`).
//!
//! Crea una base nueva con todas las migraciones indicadas y la borra al final. Necesita la
//! variable `PRUEBAS_DATABASE_URL` con una conexión con permiso para crear bases de datos,
//! por ejemplo `postgres://postgres@127.0.0.1:5432/postgres`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{AssertSqlSafe, PgPool};

use super::{OrigenMigraciones, aplicar_migraciones, combinar_migraciones, conectar};
use crate::configuracion::Secreto;

static CONTADOR: AtomicU32 = AtomicU32::new(0);

#[derive(Debug)]
pub struct BaseDePrueba {
    pub pool: PgPool,
    nombre: String,
    url_administracion: String,
}

impl BaseDePrueba {
    /// Crea la base y aplica las migraciones. Entra en pánico con un mensaje claro si no
    /// puede (es una utilidad de pruebas).
    pub async fn crear(origenes: &[OrigenMigraciones]) -> Self {
        let url_administracion = std::env::var("PRUEBAS_DATABASE_URL").expect(
            "Defina PRUEBAS_DATABASE_URL (p. ej. postgres://postgres@127.0.0.1:5432/postgres) para las pruebas de integración",
        );
        let marca = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or_default();
        let nombre = format!(
            "prueba_{}_{}_{}",
            std::process::id(),
            marca,
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        );

        let administracion = conectar(&Secreto::nuevo(url_administracion.clone()), 1)
            .await
            .expect("No se pudo conectar con PRUEBAS_DATABASE_URL");
        // El nombre lo genera esta función (solo letras, números y guiones bajos).
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {nombre}")))
            .execute(&administracion)
            .await
            .expect("No se pudo crear la base de datos de prueba");
        administracion.close().await;

        let url = reemplazar_base(&url_administracion, &nombre);
        let pool = conectar(&Secreto::nuevo(url), 5)
            .await
            .expect("No se pudo conectar con la base de prueba");
        let migrador = combinar_migraciones(origenes).expect("Migraciones inconsistentes");
        aplicar_migraciones(&pool, &migrador)
            .await
            .expect("No se pudieron aplicar las migraciones");
        Self {
            pool,
            nombre,
            url_administracion,
        }
    }

    /// Cierra las conexiones y borra la base.
    pub async fn eliminar(self) {
        self.pool.close().await;
        if let Ok(administracion) = conectar(&Secreto::nuevo(self.url_administracion), 1).await {
            let _ = sqlx::query(AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.nombre
            )))
            .execute(&administracion)
            .await;
            administracion.close().await;
        }
    }
}

/// Cambia el nombre de la base en una URL de conexión `postgres://…/base?opciones`.
fn reemplazar_base(url: &str, base: &str) -> String {
    let (sin_opciones, opciones) = match url.split_once('?') {
        Some((u, o)) => (u, format!("?{o}")),
        None => (url, String::new()),
    };
    let raiz = sin_opciones
        .rsplit_once('/')
        .map_or(sin_opciones, |(raiz, _)| raiz);
    format!("{raiz}/{base}{opciones}")
}

#[cfg(test)]
mod pruebas_url {
    use super::reemplazar_base;

    #[test]
    fn reemplaza_el_nombre_de_la_base() {
        assert_eq!(
            reemplazar_base("postgres://u@h:1/postgres", "x"),
            "postgres://u@h:1/x"
        );
        assert_eq!(
            reemplazar_base("postgres://u@h/postgres?sslmode=disable", "x"),
            "postgres://u@h/x?sslmode=disable"
        );
    }
}
