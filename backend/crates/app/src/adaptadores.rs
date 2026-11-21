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
