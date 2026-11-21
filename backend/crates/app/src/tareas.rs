//! Tareas programadas del worker (módulo `trabajos`): qué corre y cada cuánto. Las tareas
//! llaman a las fachadas de los módulos; `trabajos` no conoce a ninguno.

use std::sync::Arc;

use agenda::ModuloAgenda;
use async_trait::async_trait;
use chrono::{NaiveTime, TimeDelta};
use identidad::ModuloIdentidad;
use notificaciones::ModuloNotificaciones;
use nucleo::Reloj;
use sqlx::PgPool;
use trabajos::api::{ContextoTarea, ErrorTarea, Periodicidad, Programacion, Tarea};
use verificacion::ModuloVerificacion;

pub(crate) const RECORDATORIOS: &str = "agenda.recordatorios";
pub(crate) const MANTENIMIENTO: &str = "sistema.mantenimiento";

/// Recordatorios cada 5 minutos; mantenimiento a las 3:00 a. m., fuera del horario de
/// atención (RNF-DIS-02).
pub(crate) fn programaciones() -> Vec<Programacion> {
    vec![
        Programacion {
            tipo: RECORDATORIOS,
            periodicidad: Periodicidad::Cada(TimeDelta::minutes(5)),
        },
        Programacion {
            tipo: MANTENIMIENTO,
            periodicidad: Periodicidad::DiariaA(NaiveTime::from_hms_opt(3, 0, 0).unwrap_or(NaiveTime::MIN)),
        },
    ]
}

/// Lo que las tareas necesitan de los módulos.
#[derive(Debug, Clone)]
pub(crate) struct Modulos {
    pub(crate) pool: PgPool,
    pub(crate) reloj: Arc<dyn Reloj>,
    pub(crate) identidad: ModuloIdentidad,
    pub(crate) verificacion: ModuloVerificacion,
    pub(crate) agenda: ModuloAgenda,
    pub(crate) notificaciones: ModuloNotificaciones,
}

pub(crate) fn tareas(modulos: &Modulos) -> Vec<Arc<dyn Tarea>> {
    vec![
        Arc::new(Recordatorios(modulos.agenda.clone())),
        Arc::new(Mantenimiento(modulos.clone())),
    ]
}

/// Quién toma los trabajos (`tomado_por`): el contenedor y el proceso.
pub(crate) fn nombre_del_proceso() -> String {
    let equipo = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".to_owned());
    format!("{equipo}:{}", std::process::id())
}

#[derive(Debug)]
struct Recordatorios(ModuloAgenda);

#[async_trait]
impl Tarea for Recordatorios {
    fn tipo(&self) -> &'static str {
        RECORDATORIOS
    }

    async fn ejecutar(&self, _: &ContextoTarea) -> Result<String, ErrorTarea> {
        let resumen = self.0.enviar_recordatorios().await.map_err(ErrorTarea::nuevo)?;
        Ok(format!(
            "{} recordatorios encolados, {} omitidos",
            resumen.enviados, resumen.omitidos
        ))
    }
}

/// Limpieza nocturna: sesiones y enlaces vencidos, códigos viejos, enlaces de citas pasadas,
/// webhooks procesados y trabajos completados.
#[derive(Debug)]
struct Mantenimiento(Modulos);

#[async_trait]
impl Tarea for Mantenimiento {
    fn tipo(&self) -> &'static str {
        MANTENIMIENTO
    }

    async fn ejecutar(&self, _: &ContextoTarea) -> Result<String, ErrorTarea> {
        let m = &self.0;
        let ahora = m.reloj.ahora();
        let identidad = m.identidad.limpiar(ahora).await.map_err(ErrorTarea::nuevo)?;
        let codigos = m.verificacion.limpiar(ahora).await.map_err(ErrorTarea::nuevo)?;
        let enlaces = m.agenda.limpiar(ahora).await.map_err(ErrorTarea::nuevo)?;
        let webhooks = m.notificaciones.limpiar(ahora).await.map_err(ErrorTarea::nuevo)?;
        let trabajos = trabajos::limpiar_cola(&m.pool, ahora)
            .await
            .map_err(ErrorTarea::nuevo)?;
        Ok(format!(
            "borrados: {identidad} sesiones y enlaces de acceso, {codigos} códigos, {enlaces} enlaces de citas, \
             {webhooks} webhooks, {trabajos} trabajos"
        ))
    }
}
