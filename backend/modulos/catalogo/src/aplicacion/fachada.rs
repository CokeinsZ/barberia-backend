use std::sync::Arc;

use async_trait::async_trait;
use nucleo::identificadores::IdServicio;

use super::dto::ServicioDto;
use super::errores::ErrorCatalogo;
use super::puertos::Puertos;
use crate::api::{CatalogoServicios, ServicioResumen};

/// Implementación de [`CatalogoServicios`] para los demás módulos.
pub(crate) struct Fachada<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> Fachada<C> {
    pub(crate) fn nueva(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }
}

impl<C: Send> std::fmt::Debug for Fachada<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FachadaCatalogo").finish_non_exhaustive()
    }
}

#[async_trait]
impl<C: Send> CatalogoServicios<C> for Fachada<C> {
    async fn resumenes(&self, cx: &mut C, ids: &[IdServicio]) -> Result<Vec<ServicioResumen>, ErrorCatalogo> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .p
            .servicios
            .obtener(cx, ids)
            .await?
            .into_iter()
            .map(|s| ServicioResumen {
                id: s.id,
                tipo: s.datos.tipo.codigo(),
                nombre: s.datos.nombre,
                precio: s.datos.precio,
                duracion_min: s.datos.duracion_min,
                activo: s.activo,
                agendable_en_linea: s.datos.agendable_en_linea,
                visible_en_landing: s.datos.visible_en_landing,
            })
            .collect())
    }

    async fn listado(&self, cx: &mut C, incluir_inactivos: bool) -> Result<Vec<ServicioDto>, ErrorCatalogo> {
        let servicios = self.p.servicios.listar(cx, incluir_inactivos).await?;
        let oferentes = self.p.oferentes.contar(cx).await?;
        Ok(servicios
            .into_iter()
            .map(|s| {
                let barberos = oferentes.get(&s.id).copied().unwrap_or_default();
                ServicioDto::desde(s, barberos)
            })
            .collect())
    }
}
