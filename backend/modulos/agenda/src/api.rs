//! Fachada pública del módulo `agenda`.

pub use crate::aplicacion::dto::{
    CitaCreadaDto, CitaDto, DetalleCitaDto, GestionCitaDto, LandingDto, MensajeCitaDto, TableroDto,
};
pub use crate::aplicacion::errores::ErrorAgenda;

/// Resultado de una pasada de la tarea de recordatorios.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ResumenRecordatorios {
    pub enviados: usize,
    /// Debidos pero no enviados: el cliente los desactivó o ya habían salido.
    pub omitidos: usize,
}

/// Cita tal como la necesita una venta (RF-VEN-02, RF-AGE-09).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitaParaCobrar {
    pub id: nucleo::identificadores::IdCita,
    pub cliente: nucleo::identificadores::IdCliente,
    pub barbero: nucleo::identificadores::IdBarbero,
    pub servicio: nucleo::identificadores::IdServicio,
    pub inicio: chrono::DateTime<chrono::Utc>,
    pub estado: &'static str,
    /// «Sin confirmar», «Confirmada» o «En curso»: todavía se puede cobrar.
    pub por_atender: bool,
}

/// Fachada para `ventas`, en la transacción de quien llama.
#[async_trait::async_trait]
pub trait CitasParaVentas<C: Send>: Send + Sync + std::fmt::Debug {
    /// La cita, bloqueada hasta el final de la transacción (nadie la cancela mientras se cobra).
    async fn para_cobrar(
        &self,
        cx: &mut C,
        cita: nucleo::identificadores::IdCita,
    ) -> Result<Option<CitaParaCobrar>, nucleo::ErrorPersistencia>;
}

/// Fachada para `clientes` (por un adaptador en `app`): citas por atender de un cliente, que
/// impiden anonimizarlo.
#[async_trait::async_trait]
pub trait CitasDeClientes<C: Send>: Send + Sync + std::fmt::Debug {
    async fn pendientes(
        &self,
        cx: &mut C,
        cliente: nucleo::identificadores::IdCliente,
    ) -> Result<i64, nucleo::ErrorPersistencia>;
}
