use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::{DateTime, Utc};
use nucleo::{Actor, ErrorDeCampo, ErrorPersistencia};

use crate::aplicacion::MODULO;
use crate::aplicacion::dto::PoliticaDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::entidades::NuevaPolitica;

#[derive(Debug, Clone)]
pub(crate) struct SolicitudPolitica {
    pub(crate) tipo: String,
    pub(crate) version: String,
    pub(crate) url: String,
    /// Sin fecha, rige desde ahora.
    pub(crate) vigente_desde: Option<DateTime<Utc>>,
}

/// Publica una versión nueva de una política legal (RNF-LEG-01). Las versiones no se editan
/// ni se borran: la autorización de cada cliente apunta a la que aceptó.
#[derive(Debug)]
pub(crate) struct PublicarPolitica<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> PublicarPolitica<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        solicitud: SolicitudPolitica,
    ) -> Result<PoliticaDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let ahora = self.p.reloj.ahora();
        let politica = NuevaPolitica::nueva(
            &solicitud.tipo,
            &solicitud.version,
            &solicitud.url,
            solicitud.vigente_desde.unwrap_or(ahora),
        )?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let id = match self.p.politicas.publicar(&mut cx, &politica).await {
            Ok(id) => id,
            Err(ErrorPersistencia::Duplicado { .. }) => {
                return Err(ErrorNegocio::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                    "version",
                    "Esa versión ya está publicada para ese tipo de política.",
                )]));
            }
            Err(otro) => return Err(otro.into()),
        };
        let dto = PoliticaDto {
            id,
            tipo: politica.tipo.codigo(),
            version: politica.version.clone(),
            url: politica.url.clone(),
            vigente_desde: politica.vigente_desde,
            vigente: politica.vigente_desde <= ahora,
        };
        self.p
            .auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "publicar_politica", "politica", id).despues(&dto),
            )
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
