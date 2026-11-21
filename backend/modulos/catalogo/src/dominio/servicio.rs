use std::collections::HashSet;

use nucleo::identificadores::{IdCategoria, IdServicio};
use nucleo::{Dinero, ErrorDeCampo, Validacion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoServicio {
    Simple,
    Combo,
}

impl TipoServicio {
    /// Valor del enum `catalogo.tipo_servicio`.
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            TipoServicio::Simple => "simple",
            TipoServicio::Combo => "combo",
        }
    }

    pub(crate) fn desde_codigo(codigo: &str) -> Option<Self> {
        match codigo {
            "simple" => Some(TipoServicio::Simple),
            "combo" => Some(TipoServicio::Combo),
            _ => None,
        }
    }
}

/// Datos tal como llegan, antes de validar.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropuestaServicio {
    pub(crate) tipo: String,
    pub(crate) categoria_id: i64,
    pub(crate) nombre: String,
    pub(crate) descripcion: String,
    pub(crate) precio: i64,
    pub(crate) duracion_min: Option<i64>,
    pub(crate) agendable_en_linea: bool,
    pub(crate) visible_en_landing: bool,
    pub(crate) destacado: bool,
    pub(crate) etiqueta: Option<String>,
    pub(crate) orden: i64,
}

/// Servicio válido: solo se construye con [`DatosServicio::nuevo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatosServicio {
    pub(crate) tipo: TipoServicio,
    pub(crate) categoria: IdCategoria,
    pub(crate) nombre: String,
    pub(crate) descripcion: String,
    pub(crate) precio: Dinero,
    pub(crate) duracion_min: Option<i16>,
    pub(crate) agendable_en_linea: bool,
    pub(crate) visible_en_landing: bool,
    pub(crate) destacado: bool,
    pub(crate) etiqueta: Option<String>,
    pub(crate) orden: i16,
}

const PRECIO_MAXIMO: i64 = 10_000_000;

impl DatosServicio {
    pub(crate) fn nuevo(p: PropuestaServicio) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let tipo = TipoServicio::desde_codigo(&p.tipo);
        if tipo.is_none() {
            v.error("tipo", "Elige servicio simple o combo.");
        }
        if p.categoria_id <= 0 {
            v.error("categoria_id", "Elige una categoría.");
        }
        let nombre = p.nombre.split_whitespace().collect::<Vec<_>>().join(" ");
        if !(2..=80).contains(&nombre.chars().count()) {
            v.error("nombre", "Escribe un nombre entre 2 y 80 caracteres.");
        }
        let descripcion = p.descripcion.trim().to_owned();
        if !(1..=300).contains(&descripcion.chars().count()) {
            v.error("descripcion", "Escribe una descripción de máximo 300 caracteres.");
        }
        let precio = Dinero::pesos(p.precio)
            .ok()
            .filter(|d| (1..=PRECIO_MAXIMO).contains(&d.valor()));
        if precio.is_none() {
            v.error(
                "precio",
                "Escribe un precio mayor que cero, en pesos y sin decimales.",
            );
        }
        let es_combo = tipo == Some(TipoServicio::Combo);
        let duracion_min = match p.duracion_min {
            Some(minutos) if (5..=480).contains(&minutos) && minutos % 5 == 0 => i16::try_from(minutos).ok(),
            Some(_) => {
                v.error(
                    "duracion_min",
                    "La duración va de 5 a 480 minutos, en múltiplos de 5.",
                );
                None
            }
            None if p.agendable_en_linea || es_combo => {
                v.error(
                    "duracion_min",
                    "Indica la duración: es obligatoria para agendar en línea y para los combos.",
                );
                None
            }
            None => None,
        };
        let etiqueta = p
            .etiqueta
            .map(|e| e.trim().to_owned())
            .filter(|e| !e.is_empty() && p.destacado);
        if etiqueta.as_ref().is_some_and(|e| e.chars().count() > 30) {
            v.error("etiqueta", "La etiqueta admite máximo 30 caracteres.");
        }
        if !(0..=999).contains(&p.orden) {
            v.error("orden", "El orden va de 0 a 999.");
        }
        v.terminar(|| Self {
            tipo: tipo.unwrap_or(TipoServicio::Simple),
            categoria: IdCategoria(p.categoria_id),
            nombre,
            descripcion,
            precio: precio.unwrap_or_default(),
            duracion_min,
            agendable_en_linea: p.agendable_en_linea,
            visible_en_landing: p.visible_en_landing,
            destacado: p.destacado,
            etiqueta,
            orden: i16::try_from(p.orden).unwrap_or_default(),
        })
    }
}

/// Reglas de forma de la composición de un combo: dos o más servicios distintos, sin incluirse
/// a sí mismo. Que existan, sean simples y estén activos lo verifica el caso de uso.
pub(crate) fn validar_componentes(
    combo: Option<IdServicio>,
    ids: &[IdServicio],
) -> Result<(), Vec<ErrorDeCampo>> {
    let distintos: HashSet<IdServicio> = ids.iter().copied().collect();
    let mut v = Validacion::default();
    if distintos.len() != ids.len() {
        v.error("componentes", "Un servicio no puede repetirse dentro del combo.");
    } else if distintos.len() < 2 {
        v.error("componentes", "Un combo agrupa al menos dos servicios simples.");
    } else if combo.is_some_and(|c| distintos.contains(&c)) {
        v.error("componentes", "Un combo no puede contenerse a sí mismo.");
    }
    v.terminar(|| ())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn corte() -> PropuestaServicio {
        PropuestaServicio {
            tipo: "simple".into(),
            categoria_id: 1,
            nombre: "  Corte   clásico ".into(),
            descripcion: "Corte con máquina y tijera.".into(),
            precio: 35_000,
            duracion_min: Some(45),
            agendable_en_linea: true,
            visible_en_landing: true,
            destacado: false,
            etiqueta: Some("Ignorada".into()),
            orden: 1,
        }
    }

    fn campos(r: Result<DatosServicio, Vec<ErrorDeCampo>>) -> Vec<String> {
        r.unwrap_err().into_iter().map(|e| e.campo).collect()
    }

    #[test]
    fn normaliza_y_valida_un_servicio_simple() {
        let datos = DatosServicio::nuevo(corte()).unwrap();
        assert_eq!(datos.nombre, "Corte clásico");
        assert_eq!(datos.etiqueta, None, "la etiqueta solo aplica a los destacados");
        let malos = PropuestaServicio {
            tipo: "otro".into(),
            categoria_id: 0,
            nombre: "C".into(),
            descripcion: " ".into(),
            precio: 0,
            duracion_min: Some(47),
            orden: 1000,
            ..corte()
        };
        assert_eq!(
            campos(DatosServicio::nuevo(malos)),
            [
                "tipo",
                "categoria_id",
                "nombre",
                "descripcion",
                "precio",
                "duracion_min",
                "orden"
            ]
        );
    }

    #[test]
    fn la_duracion_es_obligatoria_para_agendar_en_linea_y_para_los_combos() {
        let solo_en_local = PropuestaServicio {
            duracion_min: None,
            agendable_en_linea: false,
            ..corte()
        };
        assert!(DatosServicio::nuevo(solo_en_local.clone()).is_ok());
        assert_eq!(
            campos(DatosServicio::nuevo(PropuestaServicio {
                agendable_en_linea: true,
                ..solo_en_local.clone()
            })),
            ["duracion_min"]
        );
        assert_eq!(
            campos(DatosServicio::nuevo(PropuestaServicio {
                tipo: "combo".into(),
                ..solo_en_local
            })),
            ["duracion_min"]
        );
    }

    #[test]
    fn un_destacado_lleva_su_etiqueta() {
        let destacado = PropuestaServicio {
            destacado: true,
            etiqueta: Some("Ahorras $5.000".into()),
            ..corte()
        };
        assert_eq!(
            DatosServicio::nuevo(destacado).unwrap().etiqueta.as_deref(),
            Some("Ahorras $5.000")
        );
        let larga = PropuestaServicio {
            destacado: true,
            etiqueta: Some("x".repeat(31)),
            ..corte()
        };
        assert_eq!(campos(DatosServicio::nuevo(larga)), ["etiqueta"]);
    }

    #[test]
    fn un_combo_agrupa_al_menos_dos_servicios_distintos() {
        let ids = |v: &[i64]| v.iter().copied().map(IdServicio).collect::<Vec<_>>();
        assert!(validar_componentes(Some(IdServicio(9)), &ids(&[1, 2])).is_ok());
        assert!(validar_componentes(None, &ids(&[1])).is_err());
        assert!(validar_componentes(None, &ids(&[1, 1, 2])).is_err());
        assert!(validar_componentes(Some(IdServicio(9)), &ids(&[1, 9])).is_err());
    }
}
