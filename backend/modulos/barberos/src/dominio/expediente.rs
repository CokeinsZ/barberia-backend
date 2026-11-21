//! Documentos y datos privados del barbero (RF-BAR-05, RF-BAR-06, RF-BAR-18, RNF-LEG-04).

use chrono::NaiveDate;
use nucleo::{Celular, ErrorDeCampo, Validacion};

use super::datos::enumeracion;
use super::texto_opcional;

enumeracion!(TipoAdjunto {
    Identidad => "identidad",
    Contrato => "contrato",
    HojaDeVida => "hoja_de_vida",
    Certificado => "certificado",
    Afiliacion => "afiliacion",
    PlanillaPila => "planilla_pila",
});

impl TipoAdjunto {
    /// Los que forman el expediente completo (RF-BAR-06).
    pub(crate) const REQUERIDOS: [TipoAdjunto; 3] = [
        TipoAdjunto::Identidad,
        TipoAdjunto::Contrato,
        TipoAdjunto::HojaDeVida,
    ];
}

/// `AAAA-MM` → primer día del mes. Solo la planilla PILA lleva periodo, y siempre lo lleva.
pub(crate) fn periodo_planilla(
    tipo: TipoAdjunto,
    periodo: Option<&str>,
) -> Result<Option<NaiveDate>, Vec<ErrorDeCampo>> {
    match (tipo, periodo.map(str::trim).filter(|p| !p.is_empty())) {
        (TipoAdjunto::PlanillaPila, Some(texto)) => {
            NaiveDate::parse_from_str(&format!("{texto}-01"), "%Y-%m-%d")
                .map(Some)
                .map_err(|_| {
                    vec![ErrorDeCampo::nuevo(
                        "periodo",
                        "Escribe el mes de la planilla como AAAA-MM.",
                    )]
                })
        }
        (TipoAdjunto::PlanillaPila, None) => Err(vec![ErrorDeCampo::nuevo(
            "periodo",
            "Indica el mes de la planilla PILA.",
        )]),
        (_, Some(_)) => Err(vec![ErrorDeCampo::nuevo(
            "periodo",
            "Solo la planilla PILA lleva periodo.",
        )]),
        (_, None) => Ok(None),
    }
}

enumeracion!(Parentesco {
    MadrePadre => "madre_padre",
    Pareja => "pareja",
    Hermano => "hermano",
    Hijo => "hijo",
    Otro => "otro",
});

enumeracion!(MedioPago { Transferencia => "transferencia", Nequi => "nequi", Daviplata => "daviplata", Efectivo => "efectivo" });

const GRUPOS_SANGUINEOS: [&str; 8] = ["A+", "A-", "B+", "B-", "AB+", "AB-", "O+", "O-"];

/// Datos complementarios tal como llegan.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropuestaPrivados {
    pub(crate) direccion: Option<String>,
    pub(crate) ciudad: Option<String>,
    pub(crate) grupo_sanguineo: Option<String>,
    pub(crate) emergencia_nombre: Option<String>,
    pub(crate) emergencia_parentesco: Option<String>,
    pub(crate) emergencia_celular: Option<String>,
    pub(crate) eps: Option<String>,
    pub(crate) fondo_pensiones: Option<String>,
    pub(crate) arl: Option<String>,
    pub(crate) caja_compensacion: Option<String>,
    pub(crate) medio_pago: Option<String>,
    pub(crate) banco: Option<String>,
    pub(crate) tipo_cuenta: Option<String>,
    pub(crate) numero_cuenta: Option<String>,
}

/// Datos complementarios válidos. El grupo sanguíneo y el número de cuenta se cifran al
/// guardarse (RNF-LEG-04).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DatosPrivados {
    pub(crate) direccion: Option<String>,
    pub(crate) ciudad: Option<String>,
    pub(crate) grupo_sanguineo: Option<String>,
    pub(crate) emergencia_nombre: Option<String>,
    pub(crate) emergencia_parentesco: Option<&'static str>,
    pub(crate) emergencia_celular: Option<Celular>,
    pub(crate) eps: Option<String>,
    pub(crate) fondo_pensiones: Option<String>,
    pub(crate) arl: Option<String>,
    pub(crate) caja_compensacion: Option<String>,
    pub(crate) medio_pago: Option<&'static str>,
    pub(crate) banco: Option<String>,
    pub(crate) tipo_cuenta: Option<String>,
    pub(crate) numero_cuenta: Option<String>,
}

fn texto_limitado(v: &mut Validacion, campo: &str, valor: Option<String>, maximo: usize) -> Option<String> {
    let valor = texto_opcional(valor);
    if valor.as_ref().is_some_and(|t| t.chars().count() > maximo) {
        v.error(campo, format!("Admite máximo {maximo} caracteres."));
    }
    valor
}

impl DatosPrivados {
    pub(crate) fn nuevos(p: PropuestaPrivados) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let grupo_sanguineo = texto_opcional(p.grupo_sanguineo).map(|g| g.to_uppercase().replace(' ', ""));
        if grupo_sanguineo
            .as_ref()
            .is_some_and(|g| !GRUPOS_SANGUINEOS.contains(&g.as_str()))
        {
            v.error(
                "grupo_sanguineo",
                "Elige un grupo sanguíneo y RH, por ejemplo O+.",
            );
        }
        let emergencia_parentesco = match texto_opcional(p.emergencia_parentesco) {
            None => None,
            Some(codigo) => match Parentesco::desde_codigo(&codigo) {
                Some(parentesco) => Some(parentesco.codigo()),
                None => {
                    v.error(
                        "emergencia_parentesco",
                        "Elige madre o padre, pareja, hermano, hijo u otro.",
                    );
                    None
                }
            },
        };
        let emergencia_celular = match texto_opcional(p.emergencia_celular) {
            None => None,
            Some(texto) => Celular::nuevo(&texto)
                .map_err(|e| v.error("emergencia_celular", e.to_string()))
                .ok(),
        };
        let medio_pago = match texto_opcional(p.medio_pago) {
            None => None,
            Some(codigo) => match MedioPago::desde_codigo(&codigo) {
                Some(medio) => Some(medio.codigo()),
                None => {
                    v.error("medio_pago", "Elige transferencia, Nequi, Daviplata o efectivo.");
                    None
                }
            },
        };
        let numero_cuenta = texto_opcional(p.numero_cuenta).map(|n| n.replace([' ', '-'], ""));
        if numero_cuenta
            .as_ref()
            .is_some_and(|n| !(4..=30).contains(&n.len()) || !n.chars().all(|c| c.is_ascii_digit()))
        {
            v.error(
                "numero_cuenta",
                "Escribe el número de cuenta o celular de pago, solo con dígitos.",
            );
        }
        let datos = Self {
            direccion: texto_limitado(&mut v, "direccion", p.direccion, 200),
            ciudad: texto_limitado(&mut v, "ciudad", p.ciudad, 80),
            grupo_sanguineo,
            emergencia_nombre: texto_limitado(&mut v, "emergencia_nombre", p.emergencia_nombre, 120),
            emergencia_parentesco,
            emergencia_celular,
            eps: texto_limitado(&mut v, "eps", p.eps, 80),
            fondo_pensiones: texto_limitado(&mut v, "fondo_pensiones", p.fondo_pensiones, 80),
            arl: texto_limitado(&mut v, "arl", p.arl, 80),
            caja_compensacion: texto_limitado(&mut v, "caja_compensacion", p.caja_compensacion, 80),
            medio_pago,
            banco: texto_limitado(&mut v, "banco", p.banco, 80),
            tipo_cuenta: texto_limitado(&mut v, "tipo_cuenta", p.tipo_cuenta, 40),
            numero_cuenta,
        };
        v.terminar(|| datos)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_periodo_solo_aplica_a_la_planilla_pila() {
        assert_eq!(
            periodo_planilla(TipoAdjunto::PlanillaPila, Some("2026-10")).unwrap(),
            "2026-10-01".parse().ok()
        );
        assert!(periodo_planilla(TipoAdjunto::PlanillaPila, None).is_err());
        assert!(periodo_planilla(TipoAdjunto::PlanillaPila, Some("10/2026")).is_err());
        assert!(periodo_planilla(TipoAdjunto::Contrato, Some("2026-10")).is_err());
        assert_eq!(periodo_planilla(TipoAdjunto::Contrato, None).unwrap(), None);
    }

    #[test]
    fn valida_y_normaliza_los_datos_privados() {
        let datos = DatosPrivados::nuevos(PropuestaPrivados {
            grupo_sanguineo: Some("o +".into()),
            emergencia_parentesco: Some("madre_padre".into()),
            emergencia_celular: Some("300 111 2233".into()),
            medio_pago: Some("nequi".into()),
            numero_cuenta: Some("300-111-2233".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(datos.grupo_sanguineo.as_deref(), Some("O+"));
        assert_eq!(datos.numero_cuenta.as_deref(), Some("3001112233"));

        let campos: Vec<String> = DatosPrivados::nuevos(PropuestaPrivados {
            grupo_sanguineo: Some("Z+".into()),
            emergencia_parentesco: Some("vecino".into()),
            emergencia_celular: Some("123".into()),
            medio_pago: Some("bitcoin".into()),
            numero_cuenta: Some("abc".into()),
            ciudad: Some("x".repeat(81)),
            ..Default::default()
        })
        .unwrap_err()
        .into_iter()
        .map(|e| e.campo)
        .collect();
        assert_eq!(
            campos,
            [
                "grupo_sanguineo",
                "emergencia_parentesco",
                "emergencia_celular",
                "medio_pago",
                "numero_cuenta",
                "ciudad"
            ]
        );
    }
}
