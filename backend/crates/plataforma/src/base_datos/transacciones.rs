use async_trait::async_trait;
use nucleo::{ErrorPersistencia, Transacciones};
use sqlx::PgPool;

use super::{UnidadDeTrabajo, a_error_persistencia};

/// Implementación de [`Transacciones`] con PostgreSQL: cada contexto es una transacción.
#[derive(Debug, Clone)]
pub struct TransaccionesPg {
    pool: PgPool,
}

impl TransaccionesPg {
    pub fn nuevas(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Transacciones<UnidadDeTrabajo> for TransaccionesPg {
    async fn iniciar(&self) -> Result<UnidadDeTrabajo, ErrorPersistencia> {
        UnidadDeTrabajo::iniciar(&self.pool)
            .await
            .map_err(a_error_persistencia)
    }

    async fn confirmar(&self, contexto: UnidadDeTrabajo) -> Result<(), ErrorPersistencia> {
        contexto.confirmar().await.map_err(a_error_persistencia)
    }
}
