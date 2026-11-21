use nucleo::Dinero;
use nucleo::identificadores::{IdCategoria, IdServicio};
use serde::{Deserialize, Serialize};

use super::puertos::ServicioGuardado;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoriaDto {
    pub id: IdCategoria,
    pub nombre: String,
    pub orden: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponenteDto {
    pub id: IdServicio,
    pub nombre: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServicioDto {
    pub id: IdServicio,
    /// `simple` o `combo`.
    pub tipo: &'static str,
    pub categoria: CategoriaResumenDto,
    pub nombre: String,
    pub descripcion: String,
    pub precio: Dinero,
    pub duracion_min: Option<i16>,
    pub agendable_en_linea: bool,
    pub visible_en_landing: bool,
    pub activo: bool,
    pub destacado: bool,
    pub etiqueta: Option<String>,
    pub orden: i16,
    /// Servicios simples que forman el combo (vacío en los simples).
    pub componentes: Vec<ComponenteDto>,
    /// Barberos vigentes que lo ofrecen (RF-SER-06).
    pub barberos_que_lo_ofrecen: i64,
    /// Aviso para la interfaz, p. ej. cuando ningún barbero lo ofrece y no se puede agendar.
    pub advertencia: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoriaResumenDto {
    pub id: IdCategoria,
    pub nombre: String,
}

impl ServicioDto {
    pub(crate) fn desde(s: ServicioGuardado, barberos: i64) -> Self {
        let advertencia = (s.activo && barberos == 0)
            .then_some("Ningún barbero ofrece este servicio: no aparece en el agendamiento.");
        Self {
            id: s.id,
            tipo: s.datos.tipo.codigo(),
            categoria: CategoriaResumenDto {
                id: s.datos.categoria,
                nombre: s.categoria_nombre,
            },
            nombre: s.datos.nombre,
            descripcion: s.datos.descripcion,
            precio: s.datos.precio,
            duracion_min: s.datos.duracion_min,
            agendable_en_linea: s.datos.agendable_en_linea,
            visible_en_landing: s.datos.visible_en_landing,
            activo: s.activo,
            destacado: s.datos.destacado,
            etiqueta: s.datos.etiqueta,
            orden: s.datos.orden,
            componentes: s.componentes,
            barberos_que_lo_ofrecen: barberos,
            advertencia,
        }
    }
}
