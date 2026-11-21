//! Implementación de la fachada [`ClientesAgenda`].

use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::EntradaAuditoria;
use nucleo::identificadores::IdCliente;
use nucleo::{Actor, Celular};

use super::casos::celular_en_uso;
use super::errores::ErrorClientes;
use super::puertos::Puertos;
use super::{ENTIDAD, MODULO};
use crate::api::{ClienteConocido, ClientesAgenda, DatosReservaEnLinea, NuevoCliente};
use crate::dominio::DatosCliente;

pub(crate) struct FachadaClientes<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for FachadaClientes<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FachadaClientes")
    }
}

#[async_trait]
impl<C: Send> ClientesAgenda<C> for FachadaClientes<C> {
    async fn por_celular(
        &self,
        cx: &mut C,
        celular: &Celular,
    ) -> Result<Option<ClienteConocido>, ErrorClientes> {
        Ok(self
            .p
            .clientes
            .por_celular(cx, celular)
            .await?
            .map(|g| g.conocido))
    }

    async fn por_ids(&self, cx: &mut C, ids: &[IdCliente]) -> Result<Vec<ClienteConocido>, ErrorClientes> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .p
            .clientes
            .por_ids(cx, ids)
            .await?
            .into_iter()
            .map(|g| g.conocido)
            .collect())
    }

    async fn registrar_en_linea(
        &self,
        cx: &mut C,
        datos: DatosReservaEnLinea,
    ) -> Result<IdCliente, ErrorClientes> {
        let validos = DatosCliente::nuevos(
            &datos.nombre,
            datos.celular.digitos(),
            datos.correo.as_deref(),
            datos.fecha_nacimiento,
            self.p.reloj.hoy(),
        )?;
        let normalizados = DatosReservaEnLinea {
            nombre: validos.nombre,
            correo: validos.correo.map(|c| c.como_str().to_owned()),
            ..datos
        };
        Ok(self
            .p
            .clientes
            .registrar_en_linea(cx, &normalizados, self.p.reloj.ahora())
            .await?)
    }

    async fn registrar_desde_panel(
        &self,
        cx: &mut C,
        actor: &Actor,
        nuevo: NuevoCliente,
    ) -> Result<IdCliente, ErrorClientes> {
        if !actor.es_personal() {
            return Err(ErrorClientes::Prohibido);
        }
        let datos = DatosCliente::nuevos(
            &nuevo.nombre,
            &nuevo.celular,
            nuevo.correo.as_deref(),
            nuevo.fecha_nacimiento,
            self.p.reloj.hoy(),
        )?;
        let id = self.p.clientes.crear(cx, &datos).await.map_err(celular_en_uso)?;
        self.p
            .auditor
            .registrar(
                cx,
                EntradaAuditoria::nueva(actor, MODULO, "crear_cliente", ENTIDAD, id),
            )
            .await?;
        Ok(id)
    }

    async fn cambiar_recordatorios(
        &self,
        cx: &mut C,
        id: IdCliente,
        activos: bool,
    ) -> Result<(), ErrorClientes> {
        Ok(self.p.clientes.cambiar_recordatorios(cx, id, activos).await?)
    }
}
