use std::sync::Arc;

use chrono::{DateTime, Days, NaiveDate, Utc};
use nucleo::{Actor, ErrorDeCampo, Pagina, SolicitudPagina, Transacciones, ZONA_HORARIA};

use super::dto::RegistroAuditoriaDto;
use super::errores::ErrorAuditoria;
use super::puertos::{ConsultaRegistros, RepositorioRegistros};

#[derive(Debug, Clone, Default)]
pub(crate) struct FiltrosAuditoria {
    pub(crate) entidad: Option<String>,
    pub(crate) entidad_id: Option<String>,
    pub(crate) desde: Option<NaiveDate>,
    pub(crate) hasta: Option<NaiveDate>,
    pub(crate) pagina: SolicitudPagina,
}

/// Historial de cambios (RF-BAR-16, RF-SIS-07). Solo administradores.
#[derive(Debug)]
pub(crate) struct ListarRegistros<C> {
    transacciones: Arc<dyn Transacciones<C>>,
    registros: Arc<dyn RepositorioRegistros<C>>,
}

impl<C: Send + 'static> ListarRegistros<C> {
    pub(crate) fn nuevo(
        transacciones: Arc<dyn Transacciones<C>>,
        registros: Arc<dyn RepositorioRegistros<C>>,
    ) -> Self {
        Self {
            transacciones,
            registros,
        }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        filtros: FiltrosAuditoria,
    ) -> Result<Pagina<RegistroAuditoriaDto>, ErrorAuditoria> {
        if !actor.es_administrador() {
            return Err(ErrorAuditoria::Prohibido);
        }
        if let (Some(desde), Some(hasta)) = (filtros.desde, filtros.hasta)
            && hasta < desde
        {
            return Err(ErrorAuditoria::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "hasta",
                "La fecha final debe ser igual o posterior a la inicial.",
            )]));
        }
        let consulta = ConsultaRegistros {
            entidad: filtros.entidad,
            entidad_id: filtros.entidad_id,
            desde: filtros.desde.and_then(inicio_del_dia),
            // `hasta` incluye todo ese día.
            hasta: filtros
                .hasta
                .and_then(|d| d.checked_add_days(Days::new(1)))
                .and_then(inicio_del_dia),
            pagina: filtros.pagina,
        };
        let mut contexto = self.transacciones.iniciar().await?;
        let (datos, total) = self.registros.listar(&mut contexto, &consulta).await?;
        self.transacciones.confirmar(contexto).await?;
        Ok(Pagina::nueva(datos, filtros.pagina, total))
    }
}

/// Medianoche de esa fecha en Bogotá, como instante UTC.
fn inicio_del_dia(fecha: NaiveDate) -> Option<DateTime<Utc>> {
    fecha
        .and_hms_opt(0, 0, 0)?
        .and_local_timezone(ZONA_HORARIA)
        .single()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod pruebas {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use nucleo::identificadores::{IdBarbero, IdUsuario};
    use nucleo::{ErrorPersistencia, SinTransaccion};

    use super::*;

    /// Guarda la última consulta recibida.
    #[derive(Debug, Default)]
    struct Registros(Mutex<Option<ConsultaRegistros>>);

    #[async_trait]
    impl RepositorioRegistros<()> for Registros {
        async fn listar(
            &self,
            _: &mut (),
            consulta: &ConsultaRegistros,
        ) -> Result<(Vec<RegistroAuditoriaDto>, i64), ErrorPersistencia> {
            *self.0.lock().unwrap() = Some(consulta.clone());
            Ok((Vec::new(), 0))
        }
    }

    fn caso() -> (ListarRegistros<()>, Arc<Registros>) {
        let registros = Arc::new(Registros::default());
        (
            ListarRegistros::nuevo(Arc::new(SinTransaccion), registros.clone()),
            registros,
        )
    }

    const ADMIN: Actor = Actor::Administrador {
        usuario: IdUsuario(1),
    };

    #[tokio::test]
    async fn solo_los_administradores_consultan_la_auditoria() {
        let (caso, _) = caso();
        let barbero = Actor::Barbero {
            usuario: IdUsuario(2),
            barbero: IdBarbero(1),
        };
        assert!(matches!(
            caso.ejecutar(&barbero, FiltrosAuditoria::default()).await,
            Err(ErrorAuditoria::Prohibido)
        ));
    }

    #[tokio::test]
    async fn las_fechas_se_interpretan_como_dias_completos_en_bogota() {
        let (caso, registros) = caso();
        let dia: NaiveDate = "2026-11-20".parse().unwrap();
        let filtros = FiltrosAuditoria {
            desde: Some(dia),
            hasta: Some(dia),
            ..Default::default()
        };
        caso.ejecutar(&ADMIN, filtros).await.unwrap();
        let consulta = registros.0.lock().unwrap().clone().unwrap();
        assert_eq!(consulta.desde, "2026-11-20T05:00:00Z".parse().ok());
        assert_eq!(consulta.hasta, "2026-11-21T05:00:00Z".parse().ok());
    }

    #[tokio::test]
    async fn rechaza_un_rango_invertido() {
        let (caso, _) = caso();
        let filtros = FiltrosAuditoria {
            desde: "2026-11-20".parse().ok(),
            hasta: "2026-11-19".parse().ok(),
            ..Default::default()
        };
        assert!(matches!(
            caso.ejecutar(&ADMIN, filtros).await,
            Err(ErrorAuditoria::DatosInvalidos(_))
        ));
    }
}
