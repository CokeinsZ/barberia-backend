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
    /// Búsqueda por nombre (sin tildes), celular o correo; los más recientes primero.
    async fn buscar(
        &self,
        cx: &mut C,
        texto: Option<&str>,
        pagina: SolicitudPagina,
    ) -> Result<(Vec<ClienteGuardado>, i64), ErrorPersistencia>;
    async fn autorizaciones(
        &self,
        cx: &mut C,
        id: IdCliente,
    ) -> Result<Vec<AutorizacionDto>, ErrorPersistencia>;
    async fn umbrales(&self, cx: &mut C) -> Result<UmbralesSegmentos, ErrorPersistencia>;
}

pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) clientes: Arc<dyn RepositorioClientes<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosClientes")
    }
}
