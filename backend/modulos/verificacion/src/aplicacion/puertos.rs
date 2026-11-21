use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use notificaciones::api::BuzonSalida;
use nucleo::{Celular, ErrorPersistencia, Reloj, Transacciones};
use plataforma::seguridad::{FirmadorUrls, Huella};

use crate::api::ClientesConocidos;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodigoGuardado {
    pub(crate) id: i64,
    pub(crate) celular: String,
    pub(crate) huella: Vec<u8>,
    pub(crate) intentos: i16,
    pub(crate) max_intentos: i16,
    pub(crate) expira_en: DateTime<Utc>,
    pub(crate) verificado: bool,
}

#[async_trait]
pub(crate) trait RepositorioCodigos<C: Send>: Send + Sync + fmt::Debug {
    /// Fecha del último código pedido para ese celular.
    async fn ultimo(&self, cx: &mut C, celular: &Celular)
    -> Result<Option<DateTime<Utc>>, ErrorPersistencia>;
    async fn crear(
        &self,
        cx: &mut C,
        celular: &Celular,
        huella: &[u8; 32],
        expira_en: DateTime<Utc>,
        ip: Option<IpAddr>,
        ahora: DateTime<Utc>,
    ) -> Result<i64, ErrorPersistencia>;
    /// Con la fila bloqueada hasta el fin de la transacción (intentos simultáneos).
    async fn para_confirmar(&self, cx: &mut C, id: i64) -> Result<Option<CodigoGuardado>, ErrorPersistencia>;
    /// Devuelve los intentos ya hechos.
    async fn registrar_fallo(&self, cx: &mut C, id: i64) -> Result<i16, ErrorPersistencia>;
    async fn marcar_verificado(
        &self,
        cx: &mut C,
        id: i64,
        comprobante: &Huella,
        comprobante_expira_en: DateTime<Utc>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
    /// Marca el comprobante como usado si estaba vigente y devuelve su celular.
    async fn consumir_comprobante(
        &self,
        cx: &mut C,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<String>, ErrorPersistencia>;
}

pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) codigos: Arc<dyn RepositorioCodigos<C>>,
    pub(crate) buzon: Arc<dyn BuzonSalida<C>>,
    pub(crate) clientes: Arc<dyn ClientesConocidos<C>>,
    /// HMAC de los códigos con la clave del servidor.
    pub(crate) firmador: FirmadorUrls,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosVerificacion")
    }
}
