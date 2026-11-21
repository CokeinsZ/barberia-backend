//! Dobles de prueba compartidos por los casos de uso del módulo.

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nucleo::ErrorPersistencia;
use serde::Serialize;

use super::puertos::{Canal, MensajePendiente, RepositorioMensajes};
use crate::api::NuevoAviso;

/// Cola en memoria: registra qué pasó con cada mensaje.
#[derive(Debug, Default)]
pub(crate) struct MensajesEnMemoria {
    pub(crate) pendientes: Mutex<Vec<MensajePendiente>>,
    /// `(id, id_proveedor, conservar_variables)`.
    pub(crate) enviados: Mutex<Vec<(i64, Option<String>, bool)>>,
    pub(crate) reintentos: Mutex<Vec<(i64, DateTime<Utc>)>>,
    /// `(id, error, aviso)`.
    pub(crate) fallidos: Mutex<Vec<(i64, String, Option<NuevoAviso>)>>,
}

#[async_trait]
impl RepositorioMensajes for MensajesEnMemoria {
    async fn reservar(
        &self,
        _: Canal,
        _: DateTime<Utc>,
        _: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<MensajePendiente>, ErrorPersistencia> {
        let mut pendientes = self.pendientes.lock().unwrap();
        let n = usize::try_from(limite).unwrap().min(pendientes.len());
        Ok(pendientes.drain(..n).collect())
    }

    async fn marcar_enviado(
        &self,
        id: i64,
        _: DateTime<Utc>,
        id_proveedor: Option<&str>,
        conservar_variables: bool,
    ) -> Result<(), ErrorPersistencia> {
        self.enviados
            .lock()
            .unwrap()
            .push((id, id_proveedor.map(str::to_owned), conservar_variables));
        Ok(())
    }

    async fn reintentar(&self, id: i64, _: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        self.reintentos.lock().unwrap().push((id, cuando));
        Ok(())
    }

    async fn marcar_fallido(
        &self,
        id: i64,
        _: DateTime<Utc>,
        error: &str,
        _: bool,
        aviso: Option<NuevoAviso>,
    ) -> Result<(), ErrorPersistencia> {
        self.fallidos.lock().unwrap().push((id, error.to_owned(), aviso));
        Ok(())
    }
}

/// Mensaje pendiente a partir de una plantilla serializable (`{ plantilla, variables }`).
pub(crate) fn pendiente(
    id: i64,
    destinatario: &str,
    plantilla: &impl Serialize,
    intentos: i16,
) -> MensajePendiente {
    let mut valor = serde_json::to_value(plantilla).unwrap();
    MensajePendiente {
        id,
        destinatario: destinatario.to_owned(),
        plantilla: valor["plantilla"].as_str().unwrap().to_owned(),
        variables: valor["variables"].take(),
        intentos,
    }
}
