//! Puertos del módulo, genéricos en el contexto transaccional `C` (ADR 0005).

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use nucleo::identificadores::IdServicio;
use nucleo::{ErrorPersistencia, Transacciones};

use super::dto::{CategoriaDto, ComponenteDto};
use super::errores::ErrorCatalogo;
use crate::api::OferentesServicio;
use crate::dominio::{DatosServicio, TipoServicio, validar_componentes};

/// Restricción única del nombre normalizado (ver la migración del módulo).
pub(crate) const RESTRICCION_NOMBRE: &str = "servicio_nombre_uk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServicioGuardado {
    pub(crate) id: IdServicio,
    pub(crate) datos: DatosServicio,
    pub(crate) activo: bool,
    pub(crate) categoria_nombre: String,
    pub(crate) componentes: Vec<ComponenteDto>,
}

#[async_trait]
pub(crate) trait RepositorioServicios<C: Send>: Send + Sync + fmt::Debug {
    async fn categorias(&self, cx: &mut C) -> Result<Vec<CategoriaDto>, ErrorPersistencia>;
    /// Ordenados por categoría, orden y nombre.
    async fn listar(
        &self,
        cx: &mut C,
        incluir_inactivos: bool,
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia>;
    async fn obtener(
        &self,
        cx: &mut C,
        ids: &[IdServicio],
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia>;
    /// `Duplicado { restriccion: "servicio_nombre_uk" }` si el nombre ya existe.
    async fn crear(&self, cx: &mut C, datos: &DatosServicio) -> Result<IdServicio, ErrorPersistencia>;
    async fn actualizar(
        &self,
        cx: &mut C,
        id: IdServicio,
        datos: &DatosServicio,
    ) -> Result<(), ErrorPersistencia>;
    async fn cambiar_estado(&self, cx: &mut C, id: IdServicio, activo: bool)
    -> Result<(), ErrorPersistencia>;
    async fn reemplazar_componentes(
        &self,
        cx: &mut C,
        combo: IdServicio,
        componentes: &[IdServicio],
    ) -> Result<(), ErrorPersistencia>;
}

#[derive(Debug)]
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) servicios: Arc<dyn RepositorioServicios<C>>,
    pub(crate) oferentes: Arc<dyn OferentesServicio<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
}

impl<C: Send> Puertos<C> {
    pub(crate) async fn validar_categoria(
        &self,
        cx: &mut C,
        datos: &DatosServicio,
    ) -> Result<(), ErrorCatalogo> {
        if self
            .servicios
            .categorias(cx)
            .await?
            .iter()
            .any(|c| c.id == datos.categoria)
        {
            Ok(())
        } else {
            Err(ErrorCatalogo::campo("categoria_id", "La categoría no existe."))
        }
    }

    /// Componentes de un combo: forma (dos o más distintos) y que existan, sean simples y estén
    /// activos.
    pub(crate) async fn validar_componentes(
        &self,
        cx: &mut C,
        combo: Option<IdServicio>,
        ids: &[IdServicio],
    ) -> Result<(), ErrorCatalogo> {
        validar_componentes(combo, ids)?;
        let encontrados = self.servicios.obtener(cx, ids).await?;
        let validos = encontrados
            .iter()
            .filter(|s| s.datos.tipo == TipoServicio::Simple && s.activo)
            .count();
        if validos != ids.len() {
            return Err(ErrorCatalogo::campo(
                "componentes",
                "Los componentes deben ser servicios simples activos.",
            ));
        }
        Ok(())
    }

    pub(crate) async fn dto(
        &self,
        cx: &mut C,
        id: IdServicio,
    ) -> Result<super::dto::ServicioDto, ErrorCatalogo> {
        let guardado = self
            .servicios
            .obtener(cx, &[id])
            .await?
            .into_iter()
            .next()
            .ok_or(ErrorCatalogo::ServicioNoEncontrado)?;
        let barberos = self
            .oferentes
            .contar(cx)
            .await?
            .get(&id)
            .copied()
            .unwrap_or_default();
        Ok(super::dto::ServicioDto::desde(guardado, barberos))
    }
}

/// Traduce el nombre repetido a su error de negocio.
pub(crate) fn nombre_en_uso(error: ErrorPersistencia) -> ErrorCatalogo {
    match error {
        ErrorPersistencia::Duplicado { restriccion } if restriccion == RESTRICCION_NOMBRE => {
            ErrorCatalogo::NombreEnUso
        }
        otro => otro.into(),
    }
}
