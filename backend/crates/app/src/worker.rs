//! Proceso de tareas en segundo plano. En la fase 0 solo verifica la conexión; el módulo
//! `trabajos` (fase 4) registra aquí la cola de mensajes, los recordatorios y el mantenimiento.

use std::time::Duration;

use plataforma::base_datos::conectar;
use plataforma::configuracion::Configuracion;

use crate::{ErrorDeArranque, senales};

pub(crate) async fn ejecutar(configuracion: &Configuracion) -> Result<(), ErrorDeArranque> {
    let pool = conectar(&configuracion.base_datos.url, 2).await?;
    tracing::info!("worker listo (aún no hay tareas registradas)");

    let mut latido = tokio::time::interval(Duration::from_secs(60));
    let apagado = senales::apagado();
    tokio::pin!(apagado);
    loop {
        tokio::select! {
            () = &mut apagado => break,
            _ = latido.tick() => {
                if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
                    tracing::warn!(error = %error, "el worker no alcanza la base de datos");
                }
            }
        }
    }
    pool.close().await;
    tracing::info!("worker detenido");
    Ok(())
}
