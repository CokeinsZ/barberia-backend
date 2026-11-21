//! Fachada para `ventas`: la cita que se cobra (RF-VEN-02, RF-AGE-09).

use std::sync::Arc;

use async_trait::async_trait;
use nucleo::ErrorPersistencia;
use nucleo::identificadores::{IdCita, IdCliente};

use super::puertos::Puertos;
use crate::api::{CitaParaCobrar, CitasDeClientes, CitasParaVentas};
use crate::dominio::cita::EstadoCita;

pub(crate) struct FachadaCitas<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for FachadaCitas<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FachadaCitas")
    }
}

#[async_trait]
impl<C: Send + 'static> CitasParaVentas<C> for FachadaCitas<C> {
    async fn para_cobrar(
        &self,
        cx: &mut C,
        cita: IdCita,
    ) -> Result<Option<CitaParaCobrar>, ErrorPersistencia> {
        let Some(c) = self.p.citas.obtener(cx, cita, true).await? else {
            return Ok(None);
        };
        Ok(Some(CitaParaCobrar {
            id: c.id,
            cliente: c.cliente,
            barbero: c.barbero,
            servicio: c.servicio,
            inicio: c.inicio,
            estado: c.estado.codigo(),
            por_atender: matches!(
                c.estado,
                EstadoCita::SinConfirmar | EstadoCita::Confirmada | EstadoCita::EnCurso
            ),
        }))
    }
}

#[async_trait]
impl<C: Send + 'static> CitasDeClientes<C> for FachadaCitas<C> {
    async fn pendientes(&self, cx: &mut C, cliente: IdCliente) -> Result<i64, ErrorPersistencia> {
        self.p.citas.pendientes_de_cliente(cx, cliente).await
    }
}
