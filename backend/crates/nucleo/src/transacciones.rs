//! Unidad de trabajo abstracta.
//!
//! Los casos de uso coordinan transacciones sin conocer la base de datos: dependen de
//! [`Transacciones<C>`], donde `C` es el contexto transaccional. En producción `C` es la
//! unidad de trabajo de PostgreSQL (`plataforma`); en las pruebas de casos de uso es `()`,
//! con [`SinTransaccion`] y repositorios en memoria.
//!
//! Los puertos que escriben reciben `&mut C`, así varios repositorios (incluidos los de otros
//! módulos, como la auditoría o la cola de correos) comparten la misma transacción.

use std::fmt;

use async_trait::async_trait;

use crate::errores::ErrorPersistencia;

#[async_trait]
pub trait Transacciones<C: Send>: Send + Sync + fmt::Debug {
    async fn iniciar(&self) -> Result<C, ErrorPersistencia>;

    async fn confirmar(&self, contexto: C) -> Result<(), ErrorPersistencia>;
}

/// Transacciones vacías para pruebas en memoria (`C = ()`).
#[derive(Debug, Default, Clone, Copy)]
pub struct SinTransaccion;

#[async_trait]
impl Transacciones<()> for SinTransaccion {
    async fn iniciar(&self) -> Result<(), ErrorPersistencia> {
        Ok(())
    }

    async fn confirmar(&self, _contexto: ()) -> Result<(), ErrorPersistencia> {
        Ok(())
    }
}
