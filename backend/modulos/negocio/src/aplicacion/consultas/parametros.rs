use std::sync::Arc;

use async_trait::async_trait;
use nucleo::Porcentaje;

use crate::api::{DatosPublicos, NegocioPublicoDto, ParametrosNegocio};
use crate::aplicacion::dto::{HorarioDiaDto, PoliticaDto};
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::puertos::Puertos;

/// Fachada [`ParametrosNegocio`] para otros módulos (p. ej. la comisión por defecto de un
/// barbero nuevo). Lee dentro del contexto de quien llama, sin política de actor.
pub(crate) struct Parametros<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> Parametros<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }
}

impl<C: Send> std::fmt::Debug for Parametros<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parametros").finish_non_exhaustive()
    }
}

#[async_trait]
impl<C: Send> ParametrosNegocio<C> for Parametros<C> {
    async fn comision_por_defecto(&self, cx: &mut C) -> Result<Porcentaje, ErrorNegocio> {
        Ok(self.p.negocio.obtener(cx).await?.porcentaje_comision_defecto())
    }
}

#[async_trait]
impl<C: Send> DatosPublicos<C> for Parametros<C> {
    async fn negocio(&self, cx: &mut C) -> Result<NegocioPublicoDto, ErrorNegocio> {
        let datos = self.p.negocio.obtener(cx).await?;
        Ok(NegocioPublicoDto {
            nombre_comercial: datos.nombre_comercial().to_owned(),
            direccion: datos.direccion().map(str::to_owned),
            zona_referencia: datos.zona_referencia().map(str::to_owned),
            whatsapp: datos.whatsapp().map(|c| c.digitos().to_owned()),
            instagram: datos.instagram().map(str::to_owned),
        })
    }

    async fn horario_atencion(&self, cx: &mut C) -> Result<Vec<HorarioDiaDto>, ErrorNegocio> {
        super::horario_atencion::calcular(&self.p, cx).await
    }

    async fn politicas_vigentes(&self, cx: &mut C) -> Result<Vec<PoliticaDto>, ErrorNegocio> {
        Ok(super::listar_politicas::con_vigencia(&self.p, cx)
            .await?
            .into_iter()
            .filter(|p| p.vigente)
            .collect())
    }
}
