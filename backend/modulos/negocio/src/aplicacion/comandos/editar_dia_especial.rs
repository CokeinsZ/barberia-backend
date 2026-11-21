use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::identificadores::IdDiaEspecial;
use nucleo::{Actor, ErrorPersistencia};

use super::registrar_dia_especial::NuevoDiaEspecial;
use crate::aplicacion::MODULO;
use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::{Puertos, RESTRICCION_FECHA_UNICA};
use crate::dominio::entidades::{DiaEspecial, TipoDiaEspecial};

/// Edita un día especial (RF-CFG-04). Con las mismas reglas del registro; si era un festivo
/// precargado, pasa a ser manual.
#[derive(Debug)]
pub(crate) struct EditarDiaEspecial<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> EditarDiaEspecial<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdDiaEspecial,
        cambios: NuevoDiaEspecial,
    ) -> Result<DiaEspecialDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let tipo = TipoDiaEspecial::desde_codigo(&cambios.tipo, cambios.hora_cierre)?;
        let dia = DiaEspecial::registrar(cambios.fecha, &cambios.nombre, tipo, self.p.reloj.hoy())?;

        let mut cx = self.p.transacciones.iniciar().await?;
        let anterior = self
            .p
            .dias
            .obtener(&mut cx, id)
            .await?
            .ok_or(ErrorNegocio::DiaEspecialNoEncontrado)?;
        match self.p.dias.actualizar(&mut cx, id, &dia).await {
            Ok(()) => {}
            Err(ErrorPersistencia::Duplicado { restriccion }) if restriccion == RESTRICCION_FECHA_UNICA => {
                return Err(ErrorNegocio::FechaOcupada(dia.fecha()));
            }
            Err(otro) => return Err(otro.into()),
        }
        let dto = DiaEspecialDto::desde(id, &dia);
        self.p
            .auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "editar_dia_especial", "dia_especial", id)
                    .antes(&DiaEspecialDto::desde(id, &anterior.dia))
                    .despues(&dto),
            )
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
