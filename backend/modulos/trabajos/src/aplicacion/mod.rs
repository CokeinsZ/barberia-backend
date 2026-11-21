//! Una pasada del worker: programar las tareas periódicas de la franja vigente y ejecutar las
//! que estén listas.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use nucleo::{ErrorPersistencia, Reloj};
use serde_json::Value;

use crate::api::{ContextoTarea, Programacion, ResumenTrabajos, Tarea};
use crate::dominio::{clave, espera_reintento, franja_vigente};

/// Trabajos que se toman por pasada; el resto queda para la siguiente.
const LOTE: usize = 20;
/// Un trabajo «en proceso» por más de este tiempo se da por abandonado (el worker se cayó).
const ABANDONO: TimeDelta = TimeDelta::minutes(15);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrabajoTomado {
    pub(crate) id: i64,
    pub(crate) tipo: String,
    pub(crate) carga: Value,
    pub(crate) ejecutar_en: DateTime<Utc>,
    /// Incluye el intento actual.
    pub(crate) intentos: i16,
    pub(crate) max_intentos: i16,
}

#[async_trait]
pub(crate) trait RepositorioTrabajos: Send + Sync + std::fmt::Debug {
    /// Inserta el trabajo si su clave no existe. Devuelve `true` si lo insertó.
    async fn programar(
        &self,
        tipo: &str,
        ejecutar_en: DateTime<Utc>,
        clave: &str,
    ) -> Result<bool, ErrorPersistencia>;
    /// Toma el siguiente trabajo listo (`FOR UPDATE SKIP LOCKED`) y lo marca en proceso.
    async fn tomar(
        &self,
        ahora: DateTime<Utc>,
        proceso: &str,
    ) -> Result<Option<TrabajoTomado>, ErrorPersistencia>;
    async fn completar(&self, id: i64, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia>;
    async fn reintentar(&self, id: i64, error: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia>;
    async fn fallar(&self, id: i64, error: &str, ahora: DateTime<Utc>) -> Result<(), ErrorPersistencia>;
    /// Devuelve a «pendiente» los trabajos tomados antes de `limite` que nunca terminaron.
    async fn recuperar_abandonados(&self, limite: DateTime<Utc>) -> Result<u64, ErrorPersistencia>;
}

#[derive(Debug)]
pub(crate) struct Ejecutor {
    pub(crate) repositorio: Arc<dyn RepositorioTrabajos>,
    pub(crate) reloj: Arc<dyn Reloj>,
    pub(crate) programaciones: Vec<Programacion>,
    pub(crate) tareas: HashMap<&'static str, Arc<dyn Tarea>>,
    /// Quién toma los trabajos (`tomado_por`), p. ej. `worker:diamante-1:42`.
    pub(crate) proceso: String,
}

impl Ejecutor {
    pub(crate) async fn pasada(&self) -> Result<ResumenTrabajos, ErrorPersistencia> {
        let mut resumen = ResumenTrabajos::default();
        let ahora = self.reloj.ahora();
        for programacion in &self.programaciones {
            let franja = franja_vigente(programacion.periodicidad, ahora);
            if self
                .repositorio
                .programar(programacion.tipo, franja, &clave(programacion.tipo, franja))
                .await?
            {
                resumen.programados += 1;
            }
        }
        let recuperados = self.repositorio.recuperar_abandonados(ahora - ABANDONO).await?;
        if recuperados > 0 {
            tracing::warn!(recuperados, "trabajos abandonados vuelven a la cola");
        }
        for _ in 0..LOTE {
            let Some(trabajo) = self.repositorio.tomar(self.reloj.ahora(), &self.proceso).await? else {
                break;
            };
            let Some(tarea) = self.tareas.get(trabajo.tipo.as_str()) else {
                tracing::error!(
                    trabajo = trabajo.id,
                    tipo = trabajo.tipo,
                    "trabajo de un tipo desconocido"
                );
                self.repositorio
                    .fallar(trabajo.id, "tipo de trabajo desconocido", self.reloj.ahora())
                    .await?;
                resumen.fallidos += 1;
                continue;
            };
            let contexto = ContextoTarea {
                programada_para: trabajo.ejecutar_en,
                carga: trabajo.carga.clone(),
            };
            match tarea.ejecutar(&contexto).await {
                Ok(detalle) => {
                    tracing::info!(
                        trabajo = trabajo.id,
                        tipo = trabajo.tipo,
                        detalle,
                        "trabajo completado"
                    );
                    self.repositorio.completar(trabajo.id, self.reloj.ahora()).await?;
                    resumen.completados += 1;
                }
                Err(error) if trabajo.intentos < trabajo.max_intentos => {
                    tracing::warn!(trabajo = trabajo.id, tipo = trabajo.tipo, error = %error, "trabajo fallido; se reintenta");
                    let cuando = self.reloj.ahora() + espera_reintento(trabajo.intentos);
                    self.repositorio.reintentar(trabajo.id, &error.0, cuando).await?;
                    resumen.reintentos += 1;
                }
                Err(error) => {
                    tracing::error!(trabajo = trabajo.id, tipo = trabajo.tipo, error = %error, "trabajo fallido definitivamente");
                    self.repositorio
                        .fallar(trabajo.id, &error.0, self.reloj.ahora())
                        .await?;
                    resumen.fallidos += 1;
                }
            }
        }
        Ok(resumen)
    }
}

#[cfg(test)]
mod pruebas {
    use std::sync::Mutex;

    use chrono::NaiveTime;
    use nucleo::RelojControlado;

    use super::*;
    use crate::api::{ErrorTarea, Periodicidad};

    /// Cola en memoria con las mismas reglas que la de PostgreSQL.
    #[derive(Debug, Default)]
    struct ColaEnMemoria {
        trabajos: Mutex<Vec<(TrabajoTomado, String, &'static str)>>,
    }

    #[async_trait]
    impl RepositorioTrabajos for ColaEnMemoria {
        async fn programar(
            &self,
            tipo: &str,
            ejecutar_en: DateTime<Utc>,
            clave: &str,
        ) -> Result<bool, ErrorPersistencia> {
            let mut trabajos = self.trabajos.lock().unwrap();
            if trabajos.iter().any(|(_, c, _)| c == clave) {
                return Ok(false);
            }
            let id = i64::try_from(trabajos.len()).unwrap() + 1;
            trabajos.push((
                TrabajoTomado {
                    id,
                    tipo: tipo.to_owned(),
                    carga: Value::Null,
                    ejecutar_en,
                    intentos: 0,
                    max_intentos: 2,
                },
                clave.to_owned(),
                "pendiente",
            ));
            Ok(true)
        }
        async fn tomar(
            &self,
            ahora: DateTime<Utc>,
            _: &str,
        ) -> Result<Option<TrabajoTomado>, ErrorPersistencia> {
            let mut trabajos = self.trabajos.lock().unwrap();
            let Some(t) = trabajos
                .iter_mut()
                .find(|(t, _, e)| *e == "pendiente" && t.ejecutar_en <= ahora)
            else {
                return Ok(None);
            };
            t.0.intentos += 1;
            t.2 = "en_proceso";
            Ok(Some(t.0.clone()))
        }
        async fn completar(&self, id: i64, _: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
            self.cambiar(id, "completado", None);
            Ok(())
        }
        async fn reintentar(&self, id: i64, _: &str, cuando: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
            self.cambiar(id, "pendiente", Some(cuando));
            Ok(())
        }
        async fn fallar(&self, id: i64, _: &str, _: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
            self.cambiar(id, "fallido", None);
            Ok(())
        }
        async fn recuperar_abandonados(&self, _: DateTime<Utc>) -> Result<u64, ErrorPersistencia> {
            Ok(0)
        }
    }

    impl ColaEnMemoria {
        fn cambiar(&self, id: i64, estado: &'static str, cuando: Option<DateTime<Utc>>) {
            let mut trabajos = self.trabajos.lock().unwrap();
            let t = trabajos.iter_mut().find(|(t, _, _)| t.id == id).unwrap();
            t.2 = estado;
            if let Some(cuando) = cuando {
                t.0.ejecutar_en = cuando;
            }
        }
        fn estados(&self) -> Vec<(String, &'static str)> {
            self.trabajos
                .lock()
                .unwrap()
                .iter()
                .map(|(_, c, e)| (c.clone(), *e))
                .collect()
        }
    }

    /// Cuenta sus ejecuciones; falla si se le pide.
    #[derive(Debug)]
    struct Contadora {
        tipo: &'static str,
        falla: bool,
        veces: Mutex<usize>,
    }

    #[async_trait]
    impl Tarea for Contadora {
        fn tipo(&self) -> &'static str {
            self.tipo
        }
        async fn ejecutar(&self, _: &ContextoTarea) -> Result<String, ErrorTarea> {
            *self.veces.lock().unwrap() += 1;
            if self.falla {
                Err(ErrorTarea::nuevo("sin base de datos"))
            } else {
                Ok("listo".into())
            }
        }
    }

    #[tokio::test]
    async fn programa_una_vez_por_franja_ejecuta_y_reintenta() {
        let cola = Arc::new(ColaEnMemoria::default());
        let reloj = Arc::new(RelojControlado::en("2026-11-21T15:02:00Z".parse().unwrap()));
        let recordatorios = Arc::new(Contadora {
            tipo: "agenda.recordatorios",
            falla: false,
            veces: Mutex::new(0),
        });
        let mantenimiento = Arc::new(Contadora {
            tipo: "sistema.mantenimiento",
            falla: true,
            veces: Mutex::new(0),
        });
        let ejecutor = Ejecutor {
            repositorio: cola.clone(),
            reloj: reloj.clone(),
            programaciones: vec![
                Programacion {
                    tipo: "agenda.recordatorios",
                    periodicidad: Periodicidad::Cada(TimeDelta::minutes(5)),
                },
                Programacion {
                    tipo: "sistema.mantenimiento",
                    periodicidad: Periodicidad::DiariaA(NaiveTime::from_hms_opt(3, 0, 0).unwrap()),
                },
            ],
            tareas: HashMap::from([
                ("agenda.recordatorios", recordatorios.clone() as Arc<dyn Tarea>),
                ("sistema.mantenimiento", mantenimiento.clone() as Arc<dyn Tarea>),
            ]),
            proceso: "prueba".into(),
        };

        let primera = ejecutor.pasada().await.unwrap();
        assert_eq!(
            primera,
            ResumenTrabajos {
                programados: 2,
                completados: 1,
                reintentos: 1,
                fallidos: 0
            }
        );
        // E2E-NOT-01: dos pasadas en la misma franja no repiten la tarea.
        let segunda = ejecutor.pasada().await.unwrap();
        assert_eq!(segunda, ResumenTrabajos::default());
        assert_eq!(*recordatorios.veces.lock().unwrap(), 1);

        // Pasado el minuto de espera, el mantenimiento se reintenta y se rinde (máximo 2 intentos);
        // los recordatorios corren en la franja siguiente.
        reloj.adelantar(TimeDelta::minutes(5));
        let tercera = ejecutor.pasada().await.unwrap();
        assert_eq!(
            (tercera.programados, tercera.completados, tercera.fallidos),
            (1, 1, 1)
        );
        assert_eq!(
            cola.estados(),
            [
                (
                    "agenda.recordatorios@2026-11-21T15:00:00Z".to_owned(),
                    "completado"
                ),
                ("sistema.mantenimiento@2026-11-21T08:00:00Z".to_owned(), "fallido"),
                (
                    "agenda.recordatorios@2026-11-21T15:05:00Z".to_owned(),
                    "completado"
                ),
            ]
        );
    }
}
