use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use nucleo::{ErrorPersistencia, Reloj};
use sqlx::PgPool;

use crate::api::{EstadoTrabajos, Programacion, ResumenTrabajos, Tarea};
use crate::aplicacion::Ejecutor;
use crate::infraestructura::{RepositorioTrabajosPg, limpiar};

#[derive(Debug, Clone)]
pub struct ModuloTrabajos {
    pool: PgPool,
    ejecutor: Arc<Ejecutor>,
}

impl ModuloTrabajos {
    /// `proceso` identifica a quien toma los trabajos (`tomado_por`).
    pub fn con_postgres(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        programaciones: Vec<Programacion>,
        tareas: Vec<Arc<dyn Tarea>>,
        proceso: String,
    ) -> Self {
        let ejecutor = Ejecutor {
            repositorio: Arc::new(RepositorioTrabajosPg::nuevo(pool.clone())),
            reloj,
            programaciones,
            tareas: tareas
                .into_iter()
                .map(|t| (t.tipo(), t))
                .collect::<HashMap<_, _>>(),
            proceso,
        };
        Self {
            pool,
            ejecutor: Arc::new(ejecutor),
        }
    }

    /// Una pasada: programa las tareas periódicas de la franja vigente y ejecuta las listas.
    pub async fn pasada(&self) -> Result<ResumenTrabajos, ErrorPersistencia> {
        self.ejecutor.pasada().await
    }

    /// Estado de la cola (RNF-OBS-02): lo usa la vigilancia.
    pub async fn estado(&self) -> Result<EstadoTrabajos, ErrorPersistencia> {
        crate::infraestructura::estado(&self.pool, self.ejecutor.reloj.ahora()).await
    }

    /// Mantenimiento: borra los trabajos completados hace más de 30 días.
    pub async fn limpiar(&self, ahora: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
        limpiar(&self.pool, ahora - TimeDelta::days(30)).await
    }
}

/// Borra los trabajos completados hace más de 30 días. Es una función aparte porque la tarea de
/// mantenimiento, que la llama, vive dentro de la misma cola.
pub async fn limpiar_cola(pool: &PgPool, ahora: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
    limpiar(pool, ahora - TimeDelta::days(30)).await
}
