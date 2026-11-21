//! Adaptadores entre módulos que no pueden depender uno del otro (DIP): los puertos los define
//! el módulo que los necesita y aquí se implementan con la fachada de otro módulo.

use std::sync::Arc;

use async_trait::async_trait;
use clientes::api::ClientesAgenda;
use nucleo::{Celular, ErrorPersistencia};
use plataforma::base_datos::UnidadDeTrabajo;
use verificacion::api::{ClienteVerificado, ClientesConocidos};

/// `verificacion` muestra los datos del cliente conocido después del código (RF-RES-08); los
/// lee de `clientes` sin depender de ese módulo.
#[derive(Debug)]
pub(crate) struct ClientesParaVerificacion(pub(crate) Arc<dyn ClientesAgenda<UnidadDeTrabajo>>);

#[async_trait]
impl ClientesConocidos<UnidadDeTrabajo> for ClientesParaVerificacion {
    async fn datos_de(
        &self,
        cx: &mut UnidadDeTrabajo,
        celular: &Celular,
    ) -> Result<Option<ClienteVerificado>, ErrorPersistencia> {
        let cliente = self
            .0
            .por_celular(cx, celular)
            .await
            .map_err(|e| ErrorPersistencia::Otro(e.to_string()))?;
        Ok(cliente.map(|c| ClienteVerificado {
            nombre: c.nombre,
            correo: c.correo,
            fecha_nacimiento: c.fecha_nacimiento,
        }))
    }
}

/// Puerto de `clientes` (citas por atender antes de anonimizar) implementado con la fachada de
/// `agenda`. Se conecta después de construir la agenda, que depende de `clientes`.
#[derive(Debug, Default)]
pub(crate) struct AgendaParaClientes(
    std::sync::OnceLock<Arc<dyn agenda::api::CitasDeClientes<UnidadDeTrabajo>>>,
);

impl AgendaParaClientes {
    pub(crate) fn conectar(&self, agenda: Arc<dyn agenda::api::CitasDeClientes<UnidadDeTrabajo>>) {
        let _ = self.0.set(agenda);
    }
}

#[async_trait]
impl clientes::api::AgendaDelCliente<UnidadDeTrabajo> for AgendaParaClientes {
    async fn citas_pendientes(
        &self,
        cx: &mut UnidadDeTrabajo,
        cliente: nucleo::identificadores::IdCliente,
    ) -> Result<i64, nucleo::ErrorPersistencia> {
        match self.0.get() {
            Some(agenda) => agenda.pendientes(cx, cliente).await,
            None => Err(nucleo::ErrorPersistencia::Otro(
                "la agenda aún no está conectada".into(),
            )),
        }
    }
}
