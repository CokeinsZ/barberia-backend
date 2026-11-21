use std::sync::Arc;

use chrono::{DateTime, Utc};
use nucleo::Reloj;
use nucleo::eventos::BusEventos;
use plataforma::base_datos::{TransaccionesPg, UnidadDeTrabajo};
use plataforma::configuracion::{ConfiguracionSmtp, ConfiguracionWhatsApp};
use sqlx::PgPool;

use crate::api::{Avisos, BuzonSalida, ConsultaMensajes, ErrorNotificaciones, ResumenEnvio, ResumenWebhooks};
use crate::aplicacion::{EnviarCorreosPendientes, EnviarWhatsAppPendientes, ProcesarWebhooks};
use crate::infraestructura::postgres::{BuzonPg, RepositorioMensajesPg, RepositorioWebhooksPg, limpiar};
use crate::infraestructura::smtp::TransporteSmtp;
use crate::infraestructura::whatsapp_simulado::WhatsAppSimulado;

/// Lo que el módulo necesita de afuera (lo arma `app`).
#[derive(Debug)]
pub struct DependenciasNotificaciones<'a> {
    pub pool: PgPool,
    pub reloj: Arc<dyn Reloj>,
    /// Sin SMTP, los correos se encolan pero no se envían.
    pub smtp: Option<&'a ConfiguracionSmtp>,
    pub whatsapp: &'a ConfiguracionWhatsApp,
    /// Para publicar las respuestas de los clientes (`RespuestaWhatsAppRecibida`).
    pub bus: Arc<BusEventos<UnidadDeTrabajo>>,
}

#[derive(Debug, Clone)]
pub struct ModuloNotificaciones {
    pool: PgPool,
    reloj: Arc<dyn Reloj>,
    buzon: Arc<BuzonPg>,
    enviar_correos: Option<Arc<EnviarCorreosPendientes>>,
    enviar_whatsapp: Option<Arc<EnviarWhatsAppPendientes>>,
    webhooks: Arc<ProcesarWebhooks<UnidadDeTrabajo>>,
    webhook: Option<Arc<plataforma::configuracion::WebhookWhatsApp>>,
}

impl ModuloNotificaciones {
    pub fn con_postgres(d: DependenciasNotificaciones<'_>) -> Result<Self, ErrorNotificaciones> {
        let mensajes = Arc::new(RepositorioMensajesPg::nuevo(d.pool.clone()));
        let enviar_correos = match d.smtp {
            Some(config) => Some(Arc::new(EnviarCorreosPendientes::nuevo(
                mensajes.clone(),
                Arc::new(TransporteSmtp::nuevo(config)?),
                d.reloj.clone(),
            ))),
            None => None,
        };
        // Sin credenciales de Meta (aún no hay adaptador real): fuera de producción se simula;
        // en producción los WhatsApp quedan en cola, como los correos sin SMTP.
        let enviar_whatsapp = d.whatsapp.simulado.then(|| {
            Arc::new(EnviarWhatsAppPendientes::nuevo(
                mensajes,
                Arc::new(WhatsAppSimulado),
                d.reloj.clone(),
            ))
        });
        let buzon = Arc::new(BuzonPg::nuevo(d.reloj.clone()));
        let webhooks = Arc::new(ProcesarWebhooks {
            transacciones: Arc::new(TransaccionesPg::nuevas(d.pool.clone())),
            webhooks: Arc::new(RepositorioWebhooksPg),
            avisos: buzon.clone(),
            bus: d.bus,
            reloj: d.reloj.clone(),
        });
        Ok(Self {
            pool: d.pool,
            reloj: d.reloj,
            buzon,
            enviar_correos,
            enviar_whatsapp,
            webhooks,
            webhook: d.whatsapp.webhook.clone().map(Arc::new),
        })
    }

    /// Buzón para los demás módulos: encola dentro de su unidad de trabajo.
    pub fn buzon(&self) -> Arc<dyn BuzonSalida<UnidadDeTrabajo>> {
        self.buzon.clone()
    }

    /// Avisos internos para la administradora, dentro de la unidad de trabajo de quien llama.
    pub fn avisos(&self) -> Arc<dyn Avisos<UnidadDeTrabajo>> {
        self.buzon.clone()
    }

    pub fn mensajes(&self) -> Arc<dyn ConsultaMensajes<UnidadDeTrabajo>> {
        self.buzon.clone()
    }

    /// Avisos, registro de mensajes y webhook de WhatsApp (bajo `/api/v1`).
    pub fn rutas(&self) -> axum::Router {
        crate::http::rutas(crate::http::Estado {
            pool: self.pool.clone(),
            reloj: self.reloj.clone(),
            webhook: self.webhook.clone(),
        })
    }

    /// `GET /_pruebas/mensajes` (bajo `/api/v1`). Solo se monta con `DIAMANTE_PRUEBAS_E2E=true`.
    pub fn rutas_de_pruebas(&self) -> axum::Router {
        crate::http::rutas_de_pruebas(self.pool.clone())
    }

    pub fn envia_correos(&self) -> bool {
        self.enviar_correos.is_some()
    }

    /// `true` si los WhatsApp salen de la cola (hoy, con el simulador).
    pub fn envia_whatsapp(&self) -> bool {
        self.enviar_whatsapp.is_some()
    }

    /// Una pasada del worker por los correos. Sin SMTP configurado no hace nada.
    pub async fn enviar_correos_pendientes(&self) -> Result<ResumenEnvio, ErrorNotificaciones> {
        match &self.enviar_correos {
            Some(caso) => Ok(caso.ejecutar().await?),
            None => Ok(ResumenEnvio::default()),
        }
    }

    /// Una pasada del worker por los WhatsApp. Sin proveedor no hace nada.
    pub async fn enviar_whatsapp_pendientes(&self) -> Result<ResumenEnvio, ErrorNotificaciones> {
        match &self.enviar_whatsapp {
            Some(caso) => Ok(caso.ejecutar().await?),
            None => Ok(ResumenEnvio::default()),
        }
    }

    /// Procesa los webhooks recibidos (estados de entrega y respuestas de los clientes).
    pub async fn procesar_webhooks(&self) -> Result<ResumenWebhooks, ErrorNotificaciones> {
        Ok(self.webhooks.ejecutar().await?)
    }

    /// Mantenimiento nocturno: borra los webhooks procesados hace más de 30 días.
    pub async fn limpiar(&self, ahora: DateTime<Utc>) -> Result<u64, ErrorNotificaciones> {
        Ok(limpiar(&self.pool, ahora).await?)
    }
}
