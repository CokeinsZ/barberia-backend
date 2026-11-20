use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Days, NaiveDate};
use nucleo::{Actor, ErrorDeCampo};

use crate::api::CalendarioNegocio;
use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;

/// Rango máximo de una consulta, para no recorrer años completos sin necesidad.
const DIAS_MAXIMOS: u64 = 400;

/// Lista festivos y cierres en un rango (RF-CFG-04). La usa el personal en el panel y el
/// módulo `agenda` (a través de [`CalendarioNegocio`]) para calcular la disponibilidad.
pub(crate) struct ListarDiasEspeciales<C: Send> {
    p: Arc<Puertos<C>>,
}

// Manual: la fachada exige `Debug` sin pedírselo al contexto `C`.
impl<C: Send> std::fmt::Debug for ListarDiasEspeciales<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListarDiasEspeciales").finish_non_exhaustive()
    }
}

impl<C: Send> ListarDiasEspeciales<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    /// Sin fechas: desde hoy y durante un año.
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        desde: Option<NaiveDate>,
        hasta: Option<NaiveDate>,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio> {
        politicas::solo_personal(actor)?;
        let desde = desde.unwrap_or_else(|| self.p.reloj.hoy());
        let hasta = hasta.unwrap_or_else(|| desde.checked_add_days(Days::new(365)).unwrap_or(desde));
        let mut cx = self.p.transacciones.iniciar().await?;
        let dias = self.consultar(&mut cx, desde, hasta).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(dias)
    }

    async fn consultar(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio> {
        if hasta < desde {
            return Err(ErrorNegocio::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "hasta",
                "La fecha final debe ser igual o posterior a la inicial.",
            )]));
        }
        if desde
            .checked_add_days(Days::new(DIAS_MAXIMOS))
            .is_some_and(|limite| hasta > limite)
        {
            return Err(ErrorNegocio::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "hasta",
                format!("Consulta como máximo {DIAS_MAXIMOS} días."),
            )]));
        }
        let guardados = self.p.dias.listar(cx, desde, hasta).await?;
        Ok(guardados
            .iter()
            .map(|g| DiaEspecialDto::desde(g.id, &g.dia))
            .collect())
    }
}

/// Fachada para otros módulos: llamada interna del sistema, sin política de actor, dentro
/// del contexto transaccional de quien llama.
#[async_trait]
impl<C: Send> CalendarioNegocio<C> for ListarDiasEspeciales<C> {
    async fn dias_especiales(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio> {
        self.consultar(cx, desde, hasta).await
    }
}
