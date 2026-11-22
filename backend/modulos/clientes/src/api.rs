//! Fachada pública del módulo `clientes`.

use std::fmt;
use std::net::IpAddr;

use async_trait::async_trait;
use chrono::NaiveDate;
use nucleo::identificadores::IdCliente;
use nucleo::{Actor, Celular};
use serde::Serialize;

pub use crate::aplicacion::dto::{ClienteFichaDto, ClienteListadoDto, ClienteRapidoDto, SolicitudTitularDto};
pub use crate::aplicacion::errores::ErrorClientes;

/// Cliente tal como lo necesita la agenda (y la verificación, para autocompletar después del
/// código).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteConocido {
    pub id: IdCliente,
    pub nombre: String,
    pub celular: String,
    pub correo: Option<String>,
    pub fecha_nacimiento: Option<NaiveDate>,
    pub visitas: i32,
    pub inasistencias: i32,
    pub recordatorios_activos: bool,
}

/// Datos de una reserva en línea (RF-RES-07, RF-RES-11). El celular ya viene verificado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosReservaEnLinea {
    pub celular: Celular,
    pub nombre: String,
    pub correo: Option<String>,
    pub fecha_nacimiento: Option<NaiveDate>,
    /// Versión de la política de tratamiento de datos aceptada.
    pub politica_version_id: i64,
    pub ip: Option<IpAddr>,
    pub agente: Option<String>,
}

/// Cliente registrado por el personal (llamada o visita).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NuevoCliente {
    pub nombre: String,
    pub celular: String,
    pub correo: Option<String>,
    pub fecha_nacimiento: Option<NaiveDate>,
}

/// Lo que `agenda` usa de los clientes, dentro de su transacción.
#[async_trait]
pub trait ClientesAgenda<C: Send>: Send + Sync + fmt::Debug {
    async fn por_celular(
        &self,
        cx: &mut C,
        celular: &Celular,
    ) -> Result<Option<ClienteConocido>, ErrorClientes>;
    async fn por_ids(&self, cx: &mut C, ids: &[IdCliente]) -> Result<Vec<ClienteConocido>, ErrorClientes>;
    /// Crea el cliente o actualiza sus datos (por celular) y guarda la prueba de la autorización.
    async fn registrar_en_linea(
        &self,
        cx: &mut C,
        datos: DatosReservaEnLinea,
    ) -> Result<IdCliente, ErrorClientes>;
    /// `CelularEnUso` si ya hay un cliente con ese celular.
    async fn registrar_desde_panel(
        &self,
        cx: &mut C,
        actor: &Actor,
        nuevo: NuevoCliente,
    ) -> Result<IdCliente, ErrorClientes>;
    /// RF-RES-18: activar o desactivar los recordatorios por WhatsApp.
    async fn cambiar_recordatorios(
        &self,
        cx: &mut C,
        id: IdCliente,
        activos: bool,
    ) -> Result<(), ErrorClientes>;
    /// Días sin volver a partir de los cuales un cliente queda «por reactivar» (DA-07).
    async fn dias_inactividad(&self, cx: &mut C) -> Result<i64, ErrorClientes>;
    /// Solicitudes del titular sin responder (consultas, correcciones y supresiones; alertas
    /// del dashboard, RF-DAS-08).
    async fn solicitudes_abiertas(&self, cx: &mut C) -> Result<i64, ErrorClientes>;
}

/// Puerto que implementa `app` con la fachada de `agenda`: antes de anonimizar a un cliente hay
/// que saber si tiene citas por atender (con el celular borrado no se le podría avisar nada).
#[async_trait]
pub trait AgendaDelCliente<C: Send>: Send + Sync + fmt::Debug {
    async fn citas_pendientes(
        &self,
        cx: &mut C,
        cliente: IdCliente,
    ) -> Result<i64, nucleo::ErrorPersistencia>;
}
