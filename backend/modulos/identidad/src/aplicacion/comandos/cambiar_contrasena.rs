use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use plataforma::seguridad::Huella;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::politicas::cuenta_propia;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::ContrasenaNueva;

/// Cambio de contraseña con sesión. Cierra las demás sesiones de la cuenta.
#[derive(Debug)]
pub(crate) struct CambiarContrasena<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> CambiarContrasena<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        sesion_actual: Option<&Huella>,
        actual: &str,
        nueva: String,
    ) -> Result<(), ErrorIdentidad> {
        let p = &self.p;
        let id = cuenta_propia(actor)?;
        let nueva = ContrasenaNueva::nueva(nueva).map_err(|e| ErrorIdentidad::campo_dominio("nueva", e))?;

        let mut cx = p.transacciones.iniciar().await?;
        let usuario = p
            .usuarios
            .por_id(&mut cx, id)
            .await?
            .ok_or(ErrorIdentidad::NoAutenticado)?;
        if !p
            .credenciales
            .verificar(actual, usuario.hash_contrasena.as_deref())
            .await
        {
            return Err(ErrorIdentidad::campo(
                "actual",
                "La contraseña actual no es correcta.",
            ));
        }
        let hash = p.credenciales.cifrar(&nueva).await?;
        let ahora = p.reloj.ahora();
        p.usuarios.guardar_contrasena(&mut cx, id, &hash).await?;
        p.sesiones
            .revocar_de_usuario(&mut cx, id, sesion_actual, ahora)
            .await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "cambiar_contrasena", ENTIDAD_USUARIO, id),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
