use std::sync::Arc;

use super::activar_cuenta::crear_contrasena_con_enlace;
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::PropositoToken;

/// RF-AUT-03: nueva contraseña desde el enlace de recuperación. Cierra todas las sesiones.
#[derive(Debug)]
pub(crate) struct RestablecerContrasena<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> RestablecerContrasena<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, token: &str, contrasena: String) -> Result<(), ErrorIdentidad> {
        crear_contrasena_con_enlace(
            &self.p,
            token,
            contrasena,
            PropositoToken::Recuperacion,
            "restablecer_contrasena",
        )
        .await
    }
}
