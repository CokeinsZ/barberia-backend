//! Datos esenciales del barbero (RF-BAR-04).

use chrono::{Datelike, NaiveDate};
use nucleo::{Celular, Correo, ErrorDeCampo, Porcentaje, Validacion};

use super::{texto, texto_opcional};

macro_rules! enumeracion {
    ($nombre:ident { $($variante:ident => $codigo:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum $nombre { $($variante),+ }

        impl $nombre {
            pub(crate) fn codigo(self) -> &'static str {
                match self { $($nombre::$variante => $codigo),+ }
            }

            pub(crate) fn desde_codigo(codigo: &str) -> Option<Self> {
                match codigo { $($codigo => Some($nombre::$variante),)+ _ => None }
            }
        }
    };
}
pub(crate) use enumeracion;

enumeracion!(TipoDocumento { Cc => "CC", Ce => "CE", Ppt => "PPT", Pa => "PA" });
enumeracion!(TipoVinculacion {
    PrestacionServicios => "prestacion_servicios",
    LaboralIndefinido => "laboral_indefinido",
    LaboralTerminoFijo => "laboral_termino_fijo",
});
enumeracion!(PeriodoLiquidacion { Semanal => "semanal", Quincenal => "quincenal", Mensual => "mensual" });

/// Datos tal como llegan.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropuestaBarbero {
    pub(crate) tipo_documento: String,
    pub(crate) numero_documento: String,
    pub(crate) nombres: String,
    pub(crate) apellidos: String,
    pub(crate) fecha_nacimiento: Option<NaiveDate>,
    pub(crate) celular: String,
    pub(crate) correo: String,
    pub(crate) especialidad: String,
    pub(crate) anios_experiencia: Option<i64>,
    pub(crate) instagram: Option<String>,
    pub(crate) biografia: Option<String>,
    pub(crate) fecha_ingreso: Option<NaiveDate>,
    pub(crate) tipo_vinculacion: String,
    pub(crate) fin_contrato: Option<NaiveDate>,
    /// Sin valor, se usa la comisión por defecto del negocio.
    pub(crate) porcentaje_comision: Option<f64>,
    pub(crate) periodo_liquidacion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatosBarbero {
    pub(crate) tipo_documento: TipoDocumento,
    pub(crate) numero_documento: String,
    pub(crate) nombres: String,
    pub(crate) apellidos: String,
    pub(crate) fecha_nacimiento: NaiveDate,
    pub(crate) celular: Celular,
    pub(crate) correo: Correo,
    pub(crate) especialidad: String,
    pub(crate) anios_experiencia: Option<i16>,
    pub(crate) instagram: Option<String>,
    pub(crate) biografia: Option<String>,
    pub(crate) fecha_ingreso: NaiveDate,
    pub(crate) tipo_vinculacion: TipoVinculacion,
    pub(crate) fin_contrato: Option<NaiveDate>,
    pub(crate) porcentaje_comision: Porcentaje,
    pub(crate) periodo_liquidacion: PeriodoLiquidacion,
}

fn edad(nacimiento: NaiveDate, hoy: NaiveDate) -> i32 {
    let mut anios = hoy.year() - nacimiento.year();
    if (hoy.month(), hoy.day()) < (nacimiento.month(), nacimiento.day()) {
        anios -= 1;
    }
    anios
}

impl DatosBarbero {
    /// Valida todo y devuelve todos los errores juntos. `comision_por_defecto` se usa si la
    /// propuesta no trae comisión.
    pub(crate) fn nuevo(
        p: PropuestaBarbero,
        hoy: NaiveDate,
        comision_por_defecto: Porcentaje,
    ) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let tipo_documento = TipoDocumento::desde_codigo(p.tipo_documento.trim());
        if tipo_documento.is_none() {
            v.error("tipo_documento", "Elige CC, CE, PPT o PA.");
        }
        let numero_documento: String = p
            .numero_documento
            .chars()
            .filter(|c| !matches!(c, '.' | ' ' | '-'))
            .collect::<String>()
            .to_uppercase();
        let numero_valido = (4..=20).contains(&numero_documento.len())
            && numero_documento.chars().all(|c| c.is_ascii_alphanumeric())
            && (tipo_documento != Some(TipoDocumento::Cc)
                || ((6..=10).contains(&numero_documento.len())
                    && numero_documento.chars().all(|c| c.is_ascii_digit())));
        if !numero_valido {
            v.error(
                "numero_documento",
                "Escribe el número de documento sin puntos (la cédula tiene de 6 a 10 dígitos).",
            );
        }
        let nombres = texto(&p.nombres);
        if !(2..=60).contains(&nombres.chars().count()) {
            v.error("nombres", "Escribe los nombres (entre 2 y 60 caracteres).");
        }
        let apellidos = texto(&p.apellidos);
        if !(2..=60).contains(&apellidos.chars().count()) {
            v.error("apellidos", "Escribe los apellidos (entre 2 y 60 caracteres).");
        }
        match p.fecha_nacimiento {
            Some(f) if (16..=90).contains(&edad(f, hoy)) => {}
            _ => v.error(
                "fecha_nacimiento",
                "Escribe una fecha de nacimiento válida (entre 16 y 90 años).",
            ),
        }
        let celular = Celular::nuevo(&p.celular)
            .map_err(|e| v.error("celular", e.to_string()))
            .ok();
        let correo = Correo::nuevo(&p.correo)
            .map_err(|e| v.error("correo", e.to_string()))
            .ok();
        let especialidad = texto(&p.especialidad);
        if !(2..=60).contains(&especialidad.chars().count()) {
            v.error(
                "especialidad",
                "Escribe la especialidad (entre 2 y 60 caracteres).",
            );
        }
        let anios_experiencia = match p.anios_experiencia {
            Some(n) if (0..=70).contains(&n) => i16::try_from(n).ok(),
            Some(_) => {
                v.error("anios_experiencia", "Los años de experiencia van de 0 a 70.");
                None
            }
            None => None,
        };
        let instagram = texto_opcional(p.instagram).map(|i| i.trim_start_matches('@').to_owned());
        if instagram.as_ref().is_some_and(|i| {
            !(1..=30).contains(&i.len())
                || !i
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
        }) {
            v.error(
                "instagram",
                "Escribe el usuario de Instagram sin espacios (letras, números, punto o guion bajo).",
            );
        }
        let biografia = texto_opcional(p.biografia);
        if biografia.as_ref().is_some_and(|b| b.chars().count() > 160) {
            v.error("biografia", "La biografía admite máximo 160 caracteres.");
        }
        let fecha_ingreso = p.fecha_ingreso;
        match fecha_ingreso {
            Some(f) if p.fecha_nacimiento.is_none_or(|n| f > n) && f <= hoy + chrono::Days::new(365) => {}
            _ => v.error("fecha_ingreso", "Escribe la fecha de ingreso."),
        }
        let tipo_vinculacion = TipoVinculacion::desde_codigo(p.tipo_vinculacion.trim());
        if tipo_vinculacion.is_none() {
            v.error(
                "tipo_vinculacion",
                "Elige prestación de servicios, laboral a término indefinido o a término fijo.",
            );
        }
        if let (Some(fin), Some(ingreso)) = (p.fin_contrato, fecha_ingreso)
            && fin < ingreso
        {
            v.error(
                "fin_contrato",
                "El fin del contrato debe ser posterior a la fecha de ingreso.",
            );
        }
        let porcentaje_comision = match p.porcentaje_comision {
            None => Some(comision_por_defecto),
            Some(valor) if (0.0..=100.0).contains(&valor) => {
                // Ya está en rango: 0..=10000 cabe en u16.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Porcentaje::desde_centesimas((valor * 100.0).round() as u16).ok()
            }
            Some(_) => None,
        };
        if porcentaje_comision.is_none() {
            v.error("porcentaje_comision", "La comisión debe estar entre 0 y 100 %.");
        }
        let periodo_liquidacion = match p.periodo_liquidacion.as_deref().map(str::trim) {
            None | Some("") => Some(PeriodoLiquidacion::Quincenal),
            Some(codigo) => PeriodoLiquidacion::desde_codigo(codigo),
        };
        if periodo_liquidacion.is_none() {
            v.error("periodo_liquidacion", "Elige semanal, quincenal o mensual.");
        }
        let errores = v.terminar(|| ());
        match (
            errores,
            tipo_documento,
            p.fecha_nacimiento,
            celular,
            correo,
            fecha_ingreso,
            tipo_vinculacion,
            porcentaje_comision,
            periodo_liquidacion,
        ) {
            (
                Ok(()),
                Some(tipo_documento),
                Some(fecha_nacimiento),
                Some(celular),
                Some(correo),
                Some(fecha_ingreso),
                Some(tipo_vinculacion),
                Some(porcentaje_comision),
                Some(periodo_liquidacion),
            ) => Ok(Self {
                tipo_documento,
                numero_documento,
                nombres,
                apellidos,
                fecha_nacimiento,
                celular,
                correo,
                especialidad,
                anios_experiencia,
                instagram,
                biografia,
                fecha_ingreso,
                tipo_vinculacion,
                fin_contrato: p.fin_contrato,
                porcentaje_comision,
                periodo_liquidacion,
            }),
            (Err(errores), ..) => Err(errores),
            _ => Err(vec![ErrorDeCampo::nuevo(
                "datos",
                "Revisa los datos del barbero.",
            )]),
        }
    }

    pub(crate) fn nombre_completo(&self) -> String {
        format!("{} {}", self.nombres, self.apellidos)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    pub(crate) fn propuesta() -> PropuestaBarbero {
        PropuestaBarbero {
            tipo_documento: "CC".into(),
            numero_documento: "1.020.304.050".into(),
            nombres: " Santiago ".into(),
            apellidos: "Ruiz  Gómez".into(),
            fecha_nacimiento: "1995-03-02".parse().ok(),
            celular: "310 555 9917".into(),
            correo: "santiago@diamante.co".into(),
            especialidad: "Fade y diseños".into(),
            anios_experiencia: Some(6),
            instagram: Some("@santi.cuts".into()),
            biografia: None,
            fecha_ingreso: "2025-01-15".parse().ok(),
            tipo_vinculacion: "prestacion_servicios".into(),
            fin_contrato: None,
            porcentaje_comision: None,
            periodo_liquidacion: None,
        }
    }

    fn hoy() -> NaiveDate {
        "2026-11-20".parse().unwrap()
    }

    fn defecto() -> Porcentaje {
        Porcentaje::entero(50).unwrap()
    }

    #[test]
    fn normaliza_los_datos_y_toma_los_valores_por_defecto() {
        let d = DatosBarbero::nuevo(propuesta(), hoy(), defecto()).unwrap();
        assert_eq!(d.numero_documento, "1020304050");
        assert_eq!(d.nombre_completo(), "Santiago Ruiz Gómez");
        assert_eq!(d.celular.digitos(), "3105559917");
        assert_eq!(d.instagram.as_deref(), Some("santi.cuts"));
        assert_eq!(d.porcentaje_comision, defecto());
        assert_eq!(d.periodo_liquidacion, PeriodoLiquidacion::Quincenal);
    }

    #[test]
    fn informa_todos_los_errores_juntos() {
        let mala = PropuestaBarbero {
            tipo_documento: "X".into(),
            numero_documento: "12".into(),
            nombres: "S".into(),
            apellidos: String::new(),
            fecha_nacimiento: "2015-01-01".parse().ok(),
            celular: "123".into(),
            correo: "no".into(),
            especialidad: "x".into(),
            anios_experiencia: Some(80),
            instagram: Some("con espacio".into()),
            biografia: Some("b".repeat(161)),
            fecha_ingreso: None,
            tipo_vinculacion: "otro".into(),
            fin_contrato: None,
            porcentaje_comision: Some(101.0),
            periodo_liquidacion: Some("diario".into()),
        };
        let campos: Vec<String> = DatosBarbero::nuevo(mala, hoy(), defecto())
            .unwrap_err()
            .into_iter()
            .map(|e| e.campo)
            .collect();
        assert_eq!(
            campos,
            [
                "tipo_documento",
                "numero_documento",
                "nombres",
                "apellidos",
                "fecha_nacimiento",
                "celular",
                "correo",
                "especialidad",
                "anios_experiencia",
                "instagram",
                "biografia",
                "fecha_ingreso",
                "tipo_vinculacion",
                "porcentaje_comision",
                "periodo_liquidacion",
            ]
        );
    }

    #[test]
    fn la_cedula_solo_tiene_digitos_y_el_contrato_termina_despues_del_ingreso() {
        let cedula_con_letras = PropuestaBarbero {
            numero_documento: "AB12345".into(),
            ..propuesta()
        };
        assert!(DatosBarbero::nuevo(cedula_con_letras, hoy(), defecto()).is_err());
        let pasaporte = PropuestaBarbero {
            tipo_documento: "PA".into(),
            numero_documento: "ab12345".into(),
            ..propuesta()
        };
        assert_eq!(
            DatosBarbero::nuevo(pasaporte, hoy(), defecto())
                .unwrap()
                .numero_documento,
            "AB12345"
        );
        let contrato = PropuestaBarbero {
            fin_contrato: "2024-12-31".parse().ok(),
            ..propuesta()
        };
        let errores = DatosBarbero::nuevo(contrato, hoy(), defecto()).unwrap_err();
        assert_eq!(errores[0].campo, "fin_contrato");
    }
}
