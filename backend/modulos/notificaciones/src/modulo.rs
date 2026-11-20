use std::sync::Arc;

use nucleo::Reloj;
use plataforma::base_datos::UnidadDeTrabajo;
use plataforma::configuracion::ConfiguracionSmtp;
use sqlx::PgPool;

use crate::api::{BuzonSalida, ErrorNotificaciones, ResumenEnvio};
use crate::aplicacion::EnviarCorreosPendientes;
use crate::infraestructura::postgres::{BuzonPg, RepositorioCorreosPg};
use crate::infraestructura::smtp::TransporteSmtp;

#[derive(Debug, Clone)]
pub struct ModuloNotificaciones {
    buzon: Arc<BuzonPg>,
    enviar_correos: Option<Arc<EnviarCorreosPendientes>>,
}

impl ModuloNotificaciones {
    /// Sin `smtp`, los correos se encolan pero no se envían (quedan pendientes).
    pub fn con_postgres(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        smtp: Option<&ConfiguracionSmtp>,
    ) -> Result<Self, ErrorNotificaciones> {
        let enviar_correos = match smtp {
            Some(config) => Some(Arc::new(EnviarCorreosPendientes::nuevo(
                Arc::new(RepositorioCorreosPg::nuevo(pool)),
                Arc::new(TransporteSmtp::nuevo(config)?),
                reloj.clone(),
            ))),
            None => None,
        };
        Ok(Self {
            buzon: Arc::new(BuzonPg::nuevo(reloj)),
            enviar_correos,
        })
    }

    /// Buzón para los demás módulos: encola dentro de su unidad de trabajo.
    pub fn buzon(&self) -> Arc<dyn BuzonSalida<UnidadDeTrabajo>> {
        self.buzon.clone()
    }

    pub fn envia_correos(&self) -> bool {
        self.enviar_correos.is_some()
    }

    /// Una pasada del worker. Sin SMTP configurado no hace nada.
    pub async fn enviar_correos_pendientes(&self) -> Result<ResumenEnvio, ErrorNotificaciones> {
        match &self.enviar_correos {
            Some(caso) => Ok(caso.ejecutar().await?),
            None => Ok(ResumenEnvio::default()),
        }
    }
}
