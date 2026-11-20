use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::{NaiveDate, NaiveTime};
use nucleo::{Actor, ErrorPersistencia};

use crate::aplicacion::MODULO;
use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::{Puertos, RESTRICCION_FECHA_UNICA};
use crate::dominio::entidades::{DiaEspecial, TipoDiaEspecial};

/// Datos de un día especial nuevo, tal como llegan.
#[derive(Debug, Clone)]
pub(crate) struct NuevoDiaEspecial {
    pub(crate) fecha: NaiveDate,
    pub(crate) nombre: String,
    pub(crate) tipo: String,
    pub(crate) hora_cierre: Option<NaiveTime>,
}

/// Registra un festivo, cierre o cierre temprano (RF-CFG-04). Solo administradores.
#[derive(Debug)]
pub(crate) struct RegistrarDiaEspecial<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> RegistrarDiaEspecial<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        nuevo: NuevoDiaEspecial,
    ) -> Result<DiaEspecialDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let tipo = TipoDiaEspecial::desde_codigo(&nuevo.tipo, nuevo.hora_cierre)?;
        let dia = DiaEspecial::registrar(nuevo.fecha, &nuevo.nombre, tipo, self.p.reloj.hoy())?;

        let mut cx = self.p.transacciones.iniciar().await?;
        let id = match self.p.dias.registrar(&mut cx, &dia).await {
            Ok(id) => id,
            Err(ErrorPersistencia::Duplicado { restriccion }) if restriccion == RESTRICCION_FECHA_UNICA => {
                return Err(ErrorNegocio::FechaOcupada(dia.fecha()));
            }
            Err(otro) => return Err(otro.into()),
        };
        let dto = DiaEspecialDto::desde(id, &dia);
        self.p
            .auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "registrar_dia_especial", "dia_especial", id)
                    .despues(&dto),
            )
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}
