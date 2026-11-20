use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::Actor;
use nucleo::identificadores::IdDiaEspecial;

use crate::aplicacion::MODULO;
use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;

/// Quita un día especial (RF-CFG-04). Solo administradores.
#[derive(Debug)]
pub(crate) struct EliminarDiaEspecial<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> EliminarDiaEspecial<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor, id: IdDiaEspecial) -> Result<(), ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let eliminado = self
            .p
            .dias
            .eliminar(&mut cx, id)
            .await?
            .ok_or(ErrorNegocio::DiaEspecialNoEncontrado)?;
        self.p
            .auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "eliminar_dia_especial", "dia_especial", id)
                    .antes(&DiaEspecialDto::desde(eliminado.id, &eliminado.dia)),
            )
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
