use std::sync::Arc;

use identidad::api::PerfilesBarberos;
use nucleo::Reloj;
use plataforma::base_datos::UnidadDeTrabajo;

use crate::infraestructura::postgres::PerfilesBarberosPg;

/// Construcción del módulo. En la fase 1 solo aporta el puerto que necesita `identidad`.
#[derive(Debug, Clone)]
pub struct ModuloBarberos {
    perfiles: Arc<PerfilesBarberosPg>,
}

impl ModuloBarberos {
    pub fn con_postgres(reloj: Arc<dyn Reloj>) -> Self {
        Self {
            perfiles: Arc::new(PerfilesBarberosPg::nuevo(reloj)),
        }
    }

    /// Datos del barbero de una cuenta, para el módulo `identidad`.
    pub fn perfiles_para_identidad(&self) -> Arc<dyn PerfilesBarberos<UnidadDeTrabajo>> {
        self.perfiles.clone()
    }
}
