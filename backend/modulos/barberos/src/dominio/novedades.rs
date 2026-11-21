//! Estado por fecha, ausencias, bloqueos y retiro (RF-BAR-11, RF-PBA-10, RN-23 a RN-25).

use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use nucleo::{ErrorDeCampo, Validacion, ZONA_HORARIA};

use super::datos::enumeracion;
use super::texto_opcional;

enumeracion!(EstadoBarbero {
    Activo => "activo",
    Vacaciones => "vacaciones",
    IncapacidadLicencia => "incapacidad_licencia",
    Suspendido => "suspendido",
    Inactivo => "inactivo",
});

impl EstadoBarbero {
    /// Ausencia temporal (cuenta en el indicador del equipo, RF-BAR-02).
    pub(crate) fn es_ausencia(self) -> bool {
        matches!(
            self,
            EstadoBarbero::Vacaciones | EstadoBarbero::IncapacidadLicencia | EstadoBarbero::Suspendido
        )
    }
}

enumeracion!(TipoAusencia {
    Vacaciones => "vacaciones",
    Incapacidad => "incapacidad",
    Licencia => "licencia",
    Suspension => "suspension",
});

enumeracion!(MotivoRetiro {
    RenunciaVoluntaria => "renuncia_voluntaria",
    TerminacionContrato => "terminacion_contrato",
    FinPrestacionServicios => "fin_prestacion_servicios",
    Otro => "otro",
});

enumeracion!(MotivoBloqueo {
    CitaMedica => "cita_medica",
    DiligenciaPersonal => "diligencia_personal",
    Capacitacion => "capacitacion",
    Otro => "otro",
});

/// Vacaciones, incapacidad, licencia o suspensión: rango de fechas (incluidas) y observación
/// obligatoria (RF-BAR-11). Las registra solo el administrador (RN-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevaAusencia {
    pub(crate) tipo: TipoAusencia,
    pub(crate) desde: NaiveDate,
    pub(crate) hasta: NaiveDate,
    pub(crate) observacion: String,
}

impl NuevaAusencia {
    pub(crate) fn nueva(
        tipo: &str,
        desde: NaiveDate,
        hasta: NaiveDate,
        observacion: &str,
    ) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let tipo_ausencia = TipoAusencia::desde_codigo(tipo.trim());
        if tipo_ausencia.is_none() {
            v.error("tipo", "Elige vacaciones, incapacidad, licencia o suspensión.");
        }
        if hasta < desde {
            v.error("hasta", "La fecha final debe ser igual o posterior a la inicial.");
        } else if (hasta - desde).num_days() > 366 {
            v.error("hasta", "Una ausencia dura como máximo un año.");
        }
        let observacion = super::texto(observacion);
        if !(3..=300).contains(&observacion.chars().count()) {
            v.error(
                "observacion",
                "Escribe una observación (entre 3 y 300 caracteres).",
            );
        }
        v.terminar(|| Self {
            tipo: tipo_ausencia.unwrap_or(TipoAusencia::Vacaciones),
            desde,
            hasta,
            observacion,
        })
    }
}

/// Bloqueo puntual de horas (RF-PBA-10): un rango dentro de un mismo día, en el futuro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevoBloqueo {
    pub(crate) inicio: DateTime<Utc>,
    pub(crate) fin: DateTime<Utc>,
    pub(crate) motivo: MotivoBloqueo,
    pub(crate) nota: Option<String>,
}

impl NuevoBloqueo {
    pub(crate) fn nuevo(
        fecha: NaiveDate,
        desde: NaiveTime,
        hasta: NaiveTime,
        motivo: &str,
        nota: Option<String>,
        ahora: DateTime<Utc>,
    ) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let motivo_bloqueo = MotivoBloqueo::desde_codigo(motivo.trim());
        if motivo_bloqueo.is_none() {
            v.error(
                "motivo",
                "Elige cita médica, diligencia personal, capacitación u otro.",
            );
        }
        if hasta <= desde {
            v.error("hasta", "La hora final debe ser posterior a la inicial.");
        }
        let local = |hora: NaiveTime| {
            fecha
                .and_time(hora)
                .and_local_timezone(ZONA_HORARIA)
                .earliest()
                .map(|d| d.with_timezone(&Utc))
        };
        let (inicio, fin) = (local(desde), local(hasta));
        if inicio.is_some_and(|i| i <= ahora) {
            v.error("desde", "El bloqueo debe empezar en el futuro.");
        }
        let nota = texto_opcional(nota);
        if nota.as_ref().is_some_and(|n| n.chars().count() > 200) {
            v.error("nota", "La nota admite máximo 200 caracteres.");
        }
        match (v.terminar(|| ()), inicio, fin, motivo_bloqueo) {
            (Ok(()), Some(inicio), Some(fin), Some(motivo)) => Ok(Self {
                inicio,
                fin,
                motivo,
                nota,
            }),
            (Err(errores), ..) => Err(errores),
            _ => Err(vec![ErrorDeCampo::nuevo("fecha", "La fecha no es válida.")]),
        }
    }
}

/// Retiro (RF-BAR-11, RN-25): fecha y motivo. El barbero deja de aparecer y de recibir citas
/// desde esa fecha, pero su historial se conserva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Retiro {
    pub(crate) fecha: NaiveDate,
    pub(crate) motivo: MotivoRetiro,
}

impl Retiro {
    pub(crate) fn nuevo(
        fecha: NaiveDate,
        motivo: &str,
        fecha_ingreso: NaiveDate,
        hoy: NaiveDate,
    ) -> Result<Self, Vec<ErrorDeCampo>> {
        let mut v = Validacion::default();
        let motivo_retiro = MotivoRetiro::desde_codigo(motivo.trim());
        if motivo_retiro.is_none() {
            v.error("motivo", "Elige renuncia voluntaria, terminación del contrato, fin de la prestación de servicios u otro.");
        }
        if fecha < fecha_ingreso {
            v.error(
                "fecha",
                "La fecha de retiro no puede ser anterior a la de ingreso.",
            );
        } else if hoy
            .checked_add_days(Days::new(365))
            .is_some_and(|limite| fecha > limite)
        {
            v.error(
                "fecha",
                "La fecha de retiro puede ser como máximo dentro de un año.",
            );
        }
        v.terminar(|| Self {
            fecha,
            motivo: motivo_retiro.unwrap_or(MotivoRetiro::Otro),
        })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn fecha(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    #[test]
    fn una_ausencia_exige_rango_y_observacion() {
        assert!(
            NuevaAusencia::nueva(
                "vacaciones",
                fecha("2026-12-01"),
                fecha("2026-12-15"),
                "Vacaciones de fin de año"
            )
            .is_ok()
        );
        let errores: Vec<String> =
            NuevaAusencia::nueva("viaje", fecha("2026-12-15"), fecha("2026-12-01"), "x")
                .unwrap_err()
                .into_iter()
                .map(|e| e.campo)
                .collect();
        assert_eq!(errores, ["tipo", "hasta", "observacion"]);
    }

    #[test]
    fn un_bloqueo_se_guarda_en_utc_y_debe_ser_futuro() {
        // 20/11/2026 a las 10:00 a. m. en Bogotá.
        let ahora: DateTime<Utc> = "2026-11-20T15:00:00Z".parse().unwrap();
        let bloqueo = NuevoBloqueo::nuevo(
            fecha("2026-11-21"),
            "14:00:00".parse().unwrap(),
            "16:00:00".parse().unwrap(),
            "cita_medica",
            None,
            ahora,
        )
        .unwrap();
        assert_eq!(
            bloqueo.inicio,
            "2026-11-21T19:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            bloqueo.fin,
            "2026-11-21T21:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );

        let pasado = NuevoBloqueo::nuevo(
            fecha("2026-11-20"),
            "09:00:00".parse().unwrap(),
            "11:00:00".parse().unwrap(),
            "otro",
            None,
            ahora,
        );
        assert_eq!(pasado.unwrap_err()[0].campo, "desde");
        let invertido = NuevoBloqueo::nuevo(
            fecha("2026-11-21"),
            "16:00:00".parse().unwrap(),
            "14:00:00".parse().unwrap(),
            "x",
            None,
            ahora,
        );
        let campos: Vec<String> = invertido.unwrap_err().into_iter().map(|e| e.campo).collect();
        assert_eq!(campos, ["motivo", "hasta"]);
    }

    #[test]
    fn el_retiro_exige_motivo_y_una_fecha_coherente() {
        let ingreso = fecha("2025-01-15");
        let hoy = fecha("2026-11-20");
        assert!(Retiro::nuevo(fecha("2026-11-30"), "renuncia_voluntaria", ingreso, hoy).is_ok());
        assert_eq!(
            Retiro::nuevo(fecha("2024-01-01"), "otro", ingreso, hoy).unwrap_err()[0].campo,
            "fecha"
        );
        assert_eq!(
            Retiro::nuevo(fecha("2026-11-30"), "despido", ingreso, hoy).unwrap_err()[0].campo,
            "motivo"
        );
    }
}
