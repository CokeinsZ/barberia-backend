//! Implementación de la fachada [`EquipoAgenda`] para `agenda`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use super::errores::ErrorBarberos;
use super::puertos::Puertos;
use crate::api::{BarberoAgenda, EquipoAgenda};

pub(crate) struct FachadaEquipo<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> FachadaEquipo<C> {
    pub(crate) fn nueva(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }
}

impl<C: Send> std::fmt::Debug for FachadaEquipo<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FachadaEquipo")
    }
}

#[async_trait]
impl<C: Send> EquipoAgenda<C> for FachadaEquipo<C> {
    async fn equipo(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<BarberoAgenda>, ErrorBarberos> {
        let filas = self.p.barberos.equipo_agenda(cx, desde, hasta).await?;
        let fotos: Vec<_> = filas.iter().filter_map(|(_, foto)| *foto).collect();
        let urls = self.p.archivos.urls_publicas(cx, &fotos).await?;
        Ok(filas
            .into_iter()
            .map(|(mut barbero, foto)| {
                barbero.foto_url = foto.and_then(|f| urls.get(&f).cloned());
                barbero
            })
            .collect())
    }

    async fn nombres(
        &self,
        cx: &mut C,
        ids: &[nucleo::identificadores::IdBarbero],
    ) -> Result<std::collections::HashMap<nucleo::identificadores::IdBarbero, String>, ErrorBarberos> {
        if ids.is_empty() {
            return Ok(Default::default());
        }
        Ok(self.p.barberos.nombres(cx, ids).await?)
    }
}
