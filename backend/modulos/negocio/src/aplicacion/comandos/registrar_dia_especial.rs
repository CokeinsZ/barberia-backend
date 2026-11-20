use std::sync::Arc;

use chrono::{NaiveDate, NaiveTime};
use nucleo::{Actor, ErrorPersistencia, Reloj};

use crate::aplicacion::dto::DiaEspecialDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::{RESTRICCION_FECHA_UNICA, RepositorioDiasEspeciales};
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
pub(crate) struct RegistrarDiaEspecial {
    dias: Arc<dyn RepositorioDiasEspeciales>,
    reloj: Arc<dyn Reloj>,
}

impl RegistrarDiaEspecial {
    pub(crate) fn nuevo(dias: Arc<dyn RepositorioDiasEspeciales>, reloj: Arc<dyn Reloj>) -> Self {
        Self { dias, reloj }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        nuevo: NuevoDiaEspecial,
    ) -> Result<DiaEspecialDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let tipo = TipoDiaEspecial::desde_codigo(&nuevo.tipo, nuevo.hora_cierre)?;
        let dia = DiaEspecial::registrar(nuevo.fecha, &nuevo.nombre, tipo, self.reloj.hoy())?;
        match self.dias.registrar(&dia).await {
            Ok(id) => Ok(DiaEspecialDto::desde(id, &dia)),
            Err(ErrorPersistencia::Duplicado { restriccion }) if restriccion == RESTRICCION_FECHA_UNICA => {
                Err(ErrorNegocio::FechaOcupada(dia.fecha()))
            }
            Err(otro) => Err(otro.into()),
        }
    }
}
