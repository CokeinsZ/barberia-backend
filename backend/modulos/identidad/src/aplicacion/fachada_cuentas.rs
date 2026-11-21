//! Implementación de la fachada [`CuentasPersonal`].

use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::EntradaAuditoria;
use nucleo::identificadores::IdUsuario;
use nucleo::{Actor, Correo, ErrorPersistencia};
use serde_json::json;

use super::comandos::{ENTIDAD_USUARIO, MODULO};
use super::dto::CuentaPersonalDto;
use super::errores::ErrorIdentidad;
use super::politicas::administrador_o_sistema;
use super::puertos::Puertos;
use crate::api::CuentasPersonal;

pub(crate) struct FachadaCuentas<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> FachadaCuentas<C> {
    pub(crate) fn nueva(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }
}

impl<C: Send> std::fmt::Debug for FachadaCuentas<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FachadaCuentas").finish_non_exhaustive()
    }
}

fn correo_en_uso(error: ErrorPersistencia) -> ErrorIdentidad {
    match error {
        ErrorPersistencia::Duplicado { .. } => ErrorIdentidad::CorreoEnUso,
        otro => otro.into(),
    }
}

#[async_trait]
impl<C: Send> CuentasPersonal<C> for FachadaCuentas<C> {
    async fn crear_cuenta_barbero(
        &self,
        cx: &mut C,
        actor: &Actor,
        correo: &Correo,
        nombre: &str,
        acceso: bool,
    ) -> Result<IdUsuario, ErrorIdentidad> {
        administrador_o_sistema(actor)?;
        let p = &self.p;
        let id = p
            .usuarios
            .crear_barbero(cx, correo, acceso, p.reloj.ahora())
            .await
            .map_err(correo_en_uso)?;
        if acceso {
            p.enviar_invitacion(cx, id, correo, nombre, "barbero").await?;
        }
        p.auditor
            .registrar(
                cx,
                EntradaAuditoria::nueva(actor, MODULO, "crear_cuenta_barbero", ENTIDAD_USUARIO, id)
                    .despues(&json!({ "correo": correo, "acceso_panel": acceso })),
            )
            .await?;
        Ok(id)
    }

    async fn cuenta(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
    ) -> Result<Option<CuentaPersonalDto>, ErrorIdentidad> {
        Ok(self.p.usuarios.resumen_cuenta(cx, usuario).await?)
    }

    async fn cambiar_correo(
        &self,
        cx: &mut C,
        actor: &Actor,
        usuario: IdUsuario,
        correo: &Correo,
        nombre: &str,
    ) -> Result<(), ErrorIdentidad> {
        administrador_o_sistema(actor)?;
        let p = &self.p;
        let cuenta = p
            .usuarios
            .resumen_cuenta(cx, usuario)
            .await?
            .ok_or(ErrorIdentidad::CuentaNoEncontrada)?;
        if cuenta.correo.eq_ignore_ascii_case(correo.como_str()) {
            return Ok(());
        }
        p.usuarios
            .cambiar_correo(cx, usuario, correo)
            .await
            .map_err(correo_en_uso)?;
        if cuenta.acceso_panel && cuenta.pendiente_activacion {
            p.enviar_invitacion(cx, usuario, correo, nombre, "barbero")
                .await?;
        }
        p.auditor
            .registrar(
                cx,
                EntradaAuditoria::nueva(actor, MODULO, "cambiar_correo", ENTIDAD_USUARIO, usuario)
                    .antes(&json!({ "correo": cuenta.correo }))
                    .despues(&json!({ "correo": correo })),
            )
            .await?;
        Ok(())
    }

    async fn cambiar_acceso(
        &self,
        cx: &mut C,
        actor: &Actor,
        usuario: IdUsuario,
        acceso: bool,
        nombre: &str,
    ) -> Result<(), ErrorIdentidad> {
        administrador_o_sistema(actor)?;
        let p = &self.p;
        let cuenta = p
            .usuarios
            .resumen_cuenta(cx, usuario)
            .await?
            .ok_or(ErrorIdentidad::CuentaNoEncontrada)?;
        let reenviar = acceso && cuenta.pendiente_activacion;
        if cuenta.acceso_panel == acceso && !reenviar {
            return Ok(());
        }
        p.usuarios.cambiar_activo(cx, usuario, acceso).await?;
        if !acceso {
            p.sesiones
                .revocar_de_usuario(cx, usuario, None, p.reloj.ahora())
                .await?;
        }
        if reenviar {
            let correo = Correo::nuevo(&cuenta.correo).map_err(|e| ErrorIdentidad::Interno(e.to_string()))?;
            p.enviar_invitacion(cx, usuario, &correo, nombre, "barbero")
                .await?;
        }
        let accion = if acceso { "dar_acceso" } else { "quitar_acceso" };
        p.auditor
            .registrar(
                cx,
                EntradaAuditoria::nueva(actor, MODULO, accion, ENTIDAD_USUARIO, usuario)
                    .antes(&json!({ "acceso_panel": cuenta.acceso_panel }))
                    .despues(&json!({ "acceso_panel": acceso, "invitacion_enviada": reenviar })),
            )
            .await?;
        Ok(())
    }
}
