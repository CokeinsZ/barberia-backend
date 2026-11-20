use chrono::{DateTime, TimeDelta, Utc};
use nucleo::Correo;
use nucleo::identificadores::IdUsuario;

use super::ErrorDominio;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Rol {
    Administrador,
    Barbero,
}

impl Rol {
    /// Valor del enum `identidad.rol`.
    pub(crate) fn como_str(self) -> &'static str {
        match self {
            Rol::Administrador => "administrador",
            Rol::Barbero => "barbero",
        }
    }

    pub(crate) fn desde_str(texto: &str) -> Option<Self> {
        match texto {
            "administrador" => Some(Rol::Administrador),
            "barbero" => Some(Rol::Barbero),
            _ => None,
        }
    }

    /// Ruta del panel al que llega cada rol después de iniciar sesión.
    pub(crate) fn panel(self) -> &'static str {
        match self {
            Rol::Administrador => "/admin",
            Rol::Barbero => "/barbero",
        }
    }
}

/// Fallos seguidos que bloquean la cuenta, y por cuánto tiempo.
pub(crate) const MAX_FALLOS: i16 = 5;
pub(crate) const DURACION_BLOQUEO: TimeDelta = TimeDelta::minutes(15);

/// Cuenta del personal con su estado de acceso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usuario {
    pub(crate) id: IdUsuario,
    pub(crate) correo: Correo,
    pub(crate) rol: Rol,
    /// Nombre del administrador. El de los barberos lo aporta su perfil.
    pub(crate) nombre_administrador: Option<String>,
    /// `None` mientras la cuenta no se ha activado.
    pub(crate) hash_contrasena: Option<String>,
    pub(crate) activo: bool,
    pub(crate) intentos_fallidos: i16,
    pub(crate) bloqueado_hasta: Option<DateTime<Utc>>,
}

/// Resultado de un intento fallido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EfectoFallo {
    Contado,
    /// Este fallo bloqueó la cuenta.
    Bloqueada,
}

impl Usuario {
    pub(crate) fn esta_bloqueado(&self, ahora: DateTime<Utc>) -> bool {
        self.bloqueado_hasta.is_some_and(|hasta| hasta > ahora)
    }

    pub(crate) fn esta_activada(&self) -> bool {
        self.hash_contrasena.is_some()
    }

    pub(crate) fn registrar_fallo(&mut self, ahora: DateTime<Utc>) -> EfectoFallo {
        self.intentos_fallidos = self.intentos_fallidos.saturating_add(1);
        if self.intentos_fallidos >= MAX_FALLOS {
            self.intentos_fallidos = 0;
            self.bloqueado_hasta = Some(ahora + DURACION_BLOQUEO);
            EfectoFallo::Bloqueada
        } else {
            EfectoFallo::Contado
        }
    }

    pub(crate) fn registrar_exito(&mut self) {
        self.intentos_fallidos = 0;
        self.bloqueado_hasta = None;
    }
}

/// Nombre de un administrador (2 a 120 caracteres, sin espacios sobrantes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NombrePersona(String);

impl NombrePersona {
    pub(crate) fn nuevo(texto: &str) -> Result<Self, ErrorDominio> {
        let limpio = texto.split_whitespace().collect::<Vec<_>>().join(" ");
        if !(2..=120).contains(&limpio.chars().count()) {
            return Err(ErrorDominio::LongitudNombre);
        }
        Ok(Self(limpio))
    }

    pub(crate) fn como_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn usuario() -> Usuario {
        Usuario {
            id: IdUsuario(1),
            correo: Correo::nuevo("laura@diamante.co").unwrap(),
            rol: Rol::Administrador,
            nombre_administrador: Some("Laura".into()),
            hash_contrasena: Some("hash".into()),
            activo: true,
            intentos_fallidos: 0,
            bloqueado_hasta: None,
        }
    }

    #[test]
    fn el_quinto_fallo_bloquea_15_minutos() {
        let ahora: DateTime<Utc> = "2026-11-20T12:00:00Z".parse().unwrap();
        let mut u = usuario();
        for _ in 0..4 {
            assert_eq!(u.registrar_fallo(ahora), EfectoFallo::Contado);
        }
        assert!(!u.esta_bloqueado(ahora));
        assert_eq!(u.registrar_fallo(ahora), EfectoFallo::Bloqueada);
        assert!(u.esta_bloqueado(ahora + TimeDelta::minutes(14)));
        assert!(!u.esta_bloqueado(ahora + TimeDelta::minutes(15)));
        assert_eq!(u.intentos_fallidos, 0);
    }

    #[test]
    fn un_acceso_correcto_reinicia_el_conteo() {
        let ahora: DateTime<Utc> = "2026-11-20T12:00:00Z".parse().unwrap();
        let mut u = usuario();
        u.registrar_fallo(ahora);
        u.registrar_exito();
        assert_eq!(u.intentos_fallidos, 0);
    }

    #[test]
    fn el_nombre_se_normaliza() {
        assert_eq!(
            NombrePersona::nuevo("  Laura   Gómez ").unwrap().como_str(),
            "Laura Gómez"
        );
        assert!(NombrePersona::nuevo("L").is_err());
    }
}
