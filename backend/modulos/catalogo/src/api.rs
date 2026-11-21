//! Fachada pública del módulo `catalogo`.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use nucleo::identificadores::IdServicio;
use nucleo::{Dinero, ErrorPersistencia};
use serde::Serialize;

pub use crate::aplicacion::dto::{CategoriaDto, ComponenteDto, ServicioDto};
pub use crate::aplicacion::errores::ErrorCatalogo;

/// Lo que otros módulos necesitan de un servicio (p. ej. `barberos` para validar los
/// servicios que ofrece un barbero; `agenda` para la duración y el precio).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServicioResumen {
    pub id: IdServicio,
    /// `simple` o `combo`.
    pub tipo: &'static str,
    pub nombre: String,
    pub precio: Dinero,
    pub duracion_min: Option<i16>,
    pub activo: bool,
    pub agendable_en_linea: bool,
    pub visible_en_landing: bool,
}

/// Consulta de servicios dentro del contexto transaccional de quien llama.
#[async_trait]
pub trait CatalogoServicios<C: Send>: Send + Sync + fmt::Debug {
    /// Los servicios que existan de la lista (los inexistentes no vienen).
    async fn resumenes(&self, cx: &mut C, ids: &[IdServicio]) -> Result<Vec<ServicioResumen>, ErrorCatalogo>;

    /// Servicios con sus componentes y cuántos barberos los ofrecen (la landing y el
    /// agendamiento filtran los activos, visibles o agendables).
    async fn listado(&self, cx: &mut C, incluir_inactivos: bool) -> Result<Vec<ServicioDto>, ErrorCatalogo>;
}

/// Puerto que implementa el módulo `barberos`: cuántos barberos vigentes ofrecen cada servicio.
#[async_trait]
pub trait OferentesServicio<C: Send>: Send + Sync + fmt::Debug {
    async fn contar(&self, cx: &mut C) -> Result<HashMap<IdServicio, i64>, ErrorPersistencia>;
}
