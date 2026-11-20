//! Proceso de tareas en segundo plano. Fase 1: envío de los correos en cola (invitaciones y
//! recuperación de contraseña). El módulo `trabajos` (fase 4) agrega los recordatorios, los
//! mensajes de WhatsApp y el mantenimiento.

use std::time::Duration;

use plataforma::configuracion::Configuracion;

use crate::composicion::Composicion;
use crate::{ErrorDeArranque, senales};

const CADA: Duration = Duration::from_secs(5);

pub(crate) async fn ejecutar(configuracion: &Configuracion) -> Result<(), ErrorDeArranque> {
    let composicion = Composicion::construir(configuracion).await?;
    let notificaciones = composicion.notificaciones.clone();
    if notificaciones.envia_correos() {
        tracing::info!(
            "worker listo: envía los correos en cola cada {} s",
            CADA.as_secs()
        );
    } else {
        tracing::warn!("worker sin SMTP (DIAMANTE_SMTP_HOST): los correos quedan en cola sin enviarse");
    }

    let mut intervalo = tokio::time::interval(CADA);
    intervalo.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let apagado = senales::apagado();
    tokio::pin!(apagado);
    loop {
        tokio::select! {
            () = &mut apagado => break,
            _ = intervalo.tick() => {
                match notificaciones.enviar_correos_pendientes().await {
                    Ok(resumen) if resumen != Default::default() => {
                        tracing::info!(
                            enviados = resumen.enviados,
                            reintentos = resumen.reintentos,
                            fallidos = resumen.fallidos,
                            "correos procesados"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(error = %error, "no se pudo procesar la cola de correos"),
                }
            }
        }
    }
    composicion.pool.close().await;
    tracing::info!("worker detenido");
    Ok(())
}
