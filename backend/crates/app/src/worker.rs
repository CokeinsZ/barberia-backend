//! Proceso de tareas en segundo plano. Cada 5 segundos hace una **pasada**: envía la cola de
//! correos y de WhatsApp, procesa los webhooks recibidos y corre la cola de trabajos
//! (recordatorios cada 5 minutos, mantenimiento a las 3:00 a. m.).
//!
//! La misma pasada la usan el worker (`diamante worker`), la API cuando
//! `DIAMANTE_WORKER_EN_API=true` (ambiente E2E, un solo reloj) y `POST /_pruebas/trabajos/ejecutar`.

use std::future::Future;
use std::time::Duration;

use notificaciones::api::{ResumenEnvio, ResumenWebhooks};
use plataforma::configuracion::Configuracion;
use serde::Serialize;
use trabajos::api::ResumenTrabajos;

use crate::composicion::Composicion;
use crate::{ErrorDeArranque, senales};

const CADA: Duration = Duration::from_secs(5);

/// Resultado de una pasada (lo devuelve `/_pruebas/trabajos/ejecutar`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct ResumenPasada {
    pub(crate) correos: ResumenEnvio,
    pub(crate) whatsapp: ResumenEnvio,
    pub(crate) webhooks: ResumenWebhooks,
    pub(crate) trabajos: ResumenTrabajos,
    /// Lo que falló en esta pasada (se reintenta en la siguiente).
    pub(crate) errores: Vec<String>,
}

impl ResumenPasada {
    fn vacia(&self) -> bool {
        *self == ResumenPasada::default()
    }
}

/// Una pasada completa. Un error en una parte no detiene las demás.
pub(crate) async fn pasada(c: &Composicion) -> ResumenPasada {
    let mut resumen = ResumenPasada::default();
    // Primero los webhooks y los trabajos: lo que encolen sale en esta misma pasada.
    match c.notificaciones.procesar_webhooks().await {
        Ok(r) => resumen.webhooks = r,
        Err(e) => resumen.errores.push(format!("webhooks: {e}")),
    }
    match c.trabajos.pasada().await {
        Ok(r) => resumen.trabajos = r,
        Err(e) => resumen.errores.push(format!("trabajos: {e}")),
    }
    match c.notificaciones.enviar_whatsapp_pendientes().await {
        Ok(r) => resumen.whatsapp = r,
        Err(e) => resumen.errores.push(format!("whatsapp: {e}")),
    }
    match c.notificaciones.enviar_correos_pendientes().await {
        Ok(r) => resumen.correos = r,
        Err(e) => resumen.errores.push(format!("correos: {e}")),
    }
    resumen
}

/// Repite la pasada cada 5 segundos hasta que `apagado` se cumpla.
pub(crate) async fn bucle(c: Composicion, apagado: impl Future<Output = ()>) {
    let mut intervalo = tokio::time::interval(CADA);
    intervalo.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tokio::pin!(apagado);
    loop {
        tokio::select! {
            () = &mut apagado => break,
            _ = intervalo.tick() => {
                let resumen = pasada(&c).await;
                if !resumen.errores.is_empty() {
                    tracing::warn!(errores = ?resumen.errores, "pasada del worker con errores");
                } else if !resumen.vacia() {
                    tracing::info!(
                        correos = ?resumen.correos,
                        whatsapp = ?resumen.whatsapp,
                        webhooks = ?resumen.webhooks,
                        trabajos = ?resumen.trabajos,
                        "pasada del worker"
                    );
                }
            }
        }
    }
}

/// Avisa en el registro qué canales quedan en cola sin enviarse.
pub(crate) fn informar_canales(c: &Composicion) {
    if !c.notificaciones.envia_correos() {
        tracing::warn!("sin SMTP (DIAMANTE_SMTP_HOST): los correos quedan en cola sin enviarse");
    }
    if c.notificaciones.envia_whatsapp() {
        tracing::info!("WhatsApp simulado: los mensajes se registran en el log y no salen a Meta");
    } else {
        tracing::warn!("sin WhatsApp de Meta configurado: los mensajes quedan en cola sin enviarse");
    }
}

pub(crate) async fn ejecutar(configuracion: &Configuracion) -> Result<(), ErrorDeArranque> {
    let composicion = Composicion::construir(configuracion).await?;
    informar_canales(&composicion);
    tracing::info!("worker listo: una pasada cada {} s", CADA.as_secs());
    bucle(composicion.clone(), senales::apagado()).await;
    composicion.pool.close().await;
    tracing::info!("worker detenido");
    Ok(())
}
