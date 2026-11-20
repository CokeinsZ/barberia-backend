//! Unidad de trabajo: una transacción que comparten los repositorios y los manejadores de
//! eventos síncronos de un mismo comando. Si algo falla, nada queda a medias.

use sqlx::{PgConnection, PgPool, Postgres, Transaction};

#[derive(Debug)]
pub struct UnidadDeTrabajo {
    transaccion: Transaction<'static, Postgres>,
}

impl UnidadDeTrabajo {
    pub async fn iniciar(pool: &PgPool) -> Result<Self, sqlx::Error> {
        Ok(Self {
            transaccion: pool.begin().await?,
        })
    }

    /// Conexión de la transacción, para ejecutar consultas dentro de ella.
    pub fn conexion(&mut self) -> &mut PgConnection {
        &mut self.transaccion
    }

    /// Serializa los cambios que afectan a una misma clave (p. ej. la agenda de un barbero)
    /// hasta el final de la transacción, con `pg_advisory_xact_lock`.
    pub async fn bloquear_clave(&mut self, espacio: i32, clave: i32) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(espacio)
            .bind(clave)
            .execute(&mut *self.transaccion)
            .await?;
        Ok(())
    }

    pub async fn confirmar(self) -> Result<(), sqlx::Error> {
        self.transaccion.commit().await
    }

    /// Revierte explícitamente (si la unidad se descarta sin confirmar, también se revierte).
    pub async fn revertir(self) -> Result<(), sqlx::Error> {
        self.transaccion.rollback().await
    }
}
