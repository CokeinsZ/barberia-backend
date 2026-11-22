//! Puertos del módulo, genéricos en el contexto transaccional `C` (ADR 0005).

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::identificadores::IdCliente;
use nucleo::{Celular, ErrorPersistencia, Reloj, SolicitudPagina, Transacciones};

use super::dto::AutorizacionDto;
use crate::api::{ClienteConocido, DatosReservaEnLinea};
use crate::dominio::{DatosCliente, UmbralesSegmentos};

pub(crate) const RESTRICCION_CELULAR: &str = "cliente_celular_uk";

/// Cliente con sus métricas (vista `clientes.v_cliente_resumen`) y datos de la ficha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClienteGuardado {
    pub(crate) conocido: ClienteConocido,
    pub(crate) total_gastado: i64,
    pub(crate) ultima_visita: Option<NaiveDate>,
    pub(crate) barbero_habitual: Option<i64>,
    pub(crate) preferencias: Option<String>,
    pub(crate) creado_en: DateTime<Utc>,
}

#[async_trait]
pub(crate) trait RepositorioClientes<C: Send>: Send + Sync + fmt::Debug {
    async fn por_celular(
        &self,
        cx: &mut C,
        celular: &Celular,
    ) -> Result<Option<ClienteGuardado>, ErrorPersistencia>;
    async fn por_ids(&self, cx: &mut C, ids: &[IdCliente])
    -> Result<Vec<ClienteGuardado>, ErrorPersistencia>;
    /// Crea o actualiza por celular y guarda la autorización (canal `en_linea`).
    async fn registrar_en_linea(
        &self,
        cx: &mut C,
        datos: &DatosReservaEnLinea,
        ahora: DateTime<Utc>,
    ) -> Result<IdCliente, ErrorPersistencia>;
    /// `Duplicado { restriccion: "cliente_celular_uk" }` si el celular ya existe.
    async fn crear(&self, cx: &mut C, datos: &DatosCliente) -> Result<IdCliente, ErrorPersistencia>;
    async fn actualizar(
        &self,
        cx: &mut C,
        id: IdCliente,
        datos: &DatosCliente,
        preferencias: Option<&str>,
        recordatorios_activos: bool,
    ) -> Result<(), ErrorPersistencia>;
    async fn cambiar_recordatorios(
        &self,
        cx: &mut C,
        id: IdCliente,
        activos: bool,
    ) -> Result<(), ErrorPersistencia>;
    /// Búsqueda por nombre (sin tildes), celular o correo; los más recientes primero. Con
    /// `pagina = None` trae todos (el segmento se calcula después y se pagina en memoria).
    async fn buscar(
        &self,
        cx: &mut C,
        texto: Option<&str>,
        pagina: Option<SolicitudPagina>,
    ) -> Result<(Vec<ClienteGuardado>, i64), ErrorPersistencia>;
    async fn autorizaciones(
        &self,
        cx: &mut C,
        id: IdCliente,
    ) -> Result<Vec<AutorizacionDto>, ErrorPersistencia>;
    async fn umbrales(&self, cx: &mut C) -> Result<UmbralesSegmentos, ErrorPersistencia>;
}

/// Solicitud del titular leída de la base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SolicitudGuardada {
    pub(crate) id: i64,
    pub(crate) cliente: Option<IdCliente>,
    pub(crate) tipo: String,
    pub(crate) descripcion: String,
    pub(crate) recibida_en: DateTime<Utc>,
    pub(crate) respondida_en: Option<DateTime<Utc>>,
    pub(crate) respondida_por: Option<i64>,
    pub(crate) respuesta: Option<String>,
}

/// Derechos del titular (Ley 1581): anonimización y solicitudes.
#[async_trait]
pub(crate) trait RepositorioTitular<C: Send>: Send + Sync + fmt::Debug {
    /// Borra los datos personales y conserva las cifras (RF-CLI-07). `false` si no existía.
    async fn anonimizar(
        &self,
        cx: &mut C,
        id: IdCliente,
        ahora: DateTime<Utc>,
    ) -> Result<bool, ErrorPersistencia>;
    async fn solicitudes(
        &self,
        cx: &mut C,
        solo_abiertas: bool,
        pagina: nucleo::SolicitudPagina,
    ) -> Result<(Vec<SolicitudGuardada>, i64), ErrorPersistencia>;
    async fn crear_solicitud(
        &self,
        cx: &mut C,
        cliente: Option<IdCliente>,
        tipo: &str,
        descripcion: &str,
        ahora: DateTime<Utc>,
    ) -> Result<SolicitudGuardada, ErrorPersistencia>;
    async fn solicitud(&self, cx: &mut C, id: i64) -> Result<Option<SolicitudGuardada>, ErrorPersistencia>;
    async fn responder_solicitud(
        &self,
        cx: &mut C,
        id: i64,
        usuario: i64,
        respuesta: &str,
        ahora: DateTime<Utc>,
    ) -> Result<Option<SolicitudGuardada>, ErrorPersistencia>;
}

pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) clientes: Arc<dyn RepositorioClientes<C>>,
    pub(crate) titular: Arc<dyn RepositorioTitular<C>>,
    pub(crate) agenda: Arc<dyn crate::api::AgendaDelCliente<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosClientes")
    }
}
