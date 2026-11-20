//! Eventos que publica el módulo `agenda`.

use chrono::{DateTime, Utc};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente};

use crate::evento;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitaCreada {
    pub cita: IdCita,
    pub cliente: IdCliente,
    pub barbero: IdBarbero,
    pub inicio: DateTime<Utc>,
}
evento!(CitaCreada, "agenda.cita_creada");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitaReprogramada {
    pub cita: IdCita,
    pub inicio_anterior: DateTime<Utc>,
    pub inicio_nuevo: DateTime<Utc>,
    pub barbero_nuevo: IdBarbero,
}
evento!(CitaReprogramada, "agenda.cita_reprogramada");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitaCancelada {
    pub cita: IdCita,
    pub tardia: bool,
    pub avisar_cliente: bool,
}
evento!(CitaCancelada, "agenda.cita_cancelada");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InasistenciaMarcada {
    pub cita: IdCita,
    pub cliente: IdCliente,
}
evento!(InasistenciaMarcada, "agenda.inasistencia_marcada");
