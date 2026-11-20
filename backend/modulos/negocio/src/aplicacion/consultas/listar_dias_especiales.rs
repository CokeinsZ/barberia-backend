use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Days, NaiveDate};
use nucleo::{Actor, ErrorDeCampo, Reloj};

use crate::api::CalendarioNegocio;
use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::RepositorioDiasEspeciales;

/// Rango máximo de una consulta, para no recorrer años completos sin necesidad.
const DIAS_MAXIMOS: u64 = 400;

/// Lista festivos y cierres en un rango (RF-CFG-04). La usa el personal en el panel y el
/// módulo `agenda` (a través de [`CalendarioNegocio`]) para calcular la disponibilidad.
#[derive(Debug)]
pub(crate) struct ListarDiasEspeciales {
    dias: Arc<dyn RepositorioDiasEspeciales>,
    reloj: Arc<dyn Reloj>,
}

impl ListarDiasEspeciales {
    pub(crate) fn nuevo(dias: Arc<dyn RepositorioDiasEspeciales>, reloj: Arc<dyn Reloj>) -> Self {
        Self { dias, reloj }
    }

    /// Sin fechas: desde hoy y durante un año.
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        desde: Option<NaiveDate>,
        hasta: Option<NaiveDate>,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio> {
        politicas::solo_personal(actor)?;
        let desde = desde.unwrap_or_else(|| self.reloj.hoy());
        let hasta = hasta.unwrap_or_else(|| desde.checked_add_days(Days::new(365)).unwrap_or(desde));
        self.consultar(desde, hasta).await
    }

    async fn consultar(
        &self,
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
        let guardados = self.dias.listar(desde, hasta).await?;
        Ok(guardados
            .iter()
            .map(|g| DiaEspecialDto::desde(g.id, &g.dia))
            .collect())
    }
}

/// Fachada para otros módulos: llamada interna del sistema, sin política de actor.
#[async_trait]
impl CalendarioNegocio for ListarDiasEspeciales {
    async fn dias_especiales(
        &self,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialDto>, ErrorNegocio> {
        self.consultar(desde, hasta).await
    }
}
