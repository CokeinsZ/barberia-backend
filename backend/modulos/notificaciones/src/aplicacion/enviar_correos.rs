use std::sync::Arc;

use chrono::TimeDelta;
use nucleo::{ErrorPersistencia, Reloj};

use super::puertos::{RepositorioCorreos, TransporteCorreo};
use crate::api::ResumenEnvio;
use crate::dominio::plantillas::renderizar;
use crate::dominio::reintentos::proximo_intento;

const TAMANO_LOTE: i64 = 20;
const RESERVA: TimeDelta = TimeDelta::minutes(5);

/// Una pasada del worker: reserva un lote de correos pendientes y los envía.
#[derive(Debug)]
pub(crate) struct EnviarCorreosPendientes {
    correos: Arc<dyn RepositorioCorreos>,
    transporte: Arc<dyn TransporteCorreo>,
    reloj: Arc<dyn Reloj>,
}

impl EnviarCorreosPendientes {
    pub(crate) fn nuevo(
        correos: Arc<dyn RepositorioCorreos>,
        transporte: Arc<dyn TransporteCorreo>,
        reloj: Arc<dyn Reloj>,
    ) -> Self {
        Self {
            correos,
            transporte,
            reloj,
        }
    }

    pub(crate) async fn ejecutar(&self) -> Result<ResumenEnvio, ErrorPersistencia> {
        let ahora = self.reloj.ahora();
        let lote = self.correos.reservar(ahora, ahora + RESERVA, TAMANO_LOTE).await?;
        let mut resumen = ResumenEnvio::default();
        for correo in lote {
            let Some(plantilla) = &correo.plantilla else {
                tracing::error!(mensaje = correo.id, "mensaje con plantilla desconocida");
                self.correos
                    .marcar_fallo(correo.id, "plantilla desconocida", None)
                    .await?;
                resumen.fallidos += 1;
                continue;
            };
            match self
                .transporte
                .enviar(&correo.destinatario, &renderizar(plantilla))
                .await
            {
                Ok(()) => {
                    self.correos.marcar_enviado(correo.id, self.reloj.ahora()).await?;
                    resumen.enviados += 1;
                }
                Err(error) => {
                    let reintento = if error.permanente {
                        None
                    } else {
                        proximo_intento(correo.intentos, self.reloj.ahora())
                    };
                    tracing::warn!(
                        mensaje = correo.id,
                        intentos = correo.intentos,
                        permanente = error.permanente,
                        error = error.mensaje,
                        "no se pudo enviar el correo"
                    );
                    self.correos
                        .marcar_fallo(correo.id, &error.mensaje, reintento)
                        .await?;
                    if reintento.is_some() {
                        resumen.reintentos += 1;
                    } else {
                        resumen.fallidos += 1;
                    }
                }
            }
        }
        Ok(resumen)
    }
}

#[cfg(test)]
mod pruebas {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use nucleo::RelojControlado;

    use super::*;
    use crate::api::PlantillaCorreo;
    use crate::aplicacion::puertos::{CorreoPendiente, ErrorTransporte};
    use crate::dominio::plantillas::CorreoRenderizado;

    #[derive(Debug, Default)]
    struct CorreosEnMemoria {
        pendientes: Mutex<Vec<CorreoPendiente>>,
        enviados: Mutex<Vec<i64>>,
        fallos: Mutex<Vec<(i64, Option<DateTime<Utc>>)>>,
    }

    #[async_trait]
    impl RepositorioCorreos for CorreosEnMemoria {
        async fn reservar(
            &self,
            _: DateTime<Utc>,
            _: DateTime<Utc>,
            limite: i64,
        ) -> Result<Vec<CorreoPendiente>, ErrorPersistencia> {
            let mut pendientes = self.pendientes.lock().unwrap();
            let n = usize::try_from(limite).unwrap().min(pendientes.len());
            Ok(pendientes.drain(..n).collect())
        }
        async fn marcar_enviado(&self, id: i64, _: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
            self.enviados.lock().unwrap().push(id);
            Ok(())
        }
        async fn marcar_fallo(
            &self,
            id: i64,
            _: &str,
            reintento: Option<DateTime<Utc>>,
        ) -> Result<(), ErrorPersistencia> {
            self.fallos.lock().unwrap().push((id, reintento));
            Ok(())
        }
    }

    /// Falla con los destinatarios que contienen «falla» (temporal) o «noexiste» (permanente).
    #[derive(Debug, Default)]
    struct TransporteDePrueba(Mutex<Vec<String>>);

    #[async_trait]
    impl TransporteCorreo for TransporteDePrueba {
        async fn enviar(
            &self,
            destinatario: &str,
            correo: &CorreoRenderizado,
        ) -> Result<(), ErrorTransporte> {
            if destinatario.contains("falla") || destinatario.contains("noexiste") {
                return Err(ErrorTransporte {
                    mensaje: "servidor SMTP no disponible".into(),
                    permanente: destinatario.contains("noexiste"),
                });
            }
            self.0
                .lock()
                .unwrap()
                .push(format!("{destinatario}: {}", correo.asunto));
            Ok(())
        }
    }

    fn pendiente(id: i64, destinatario: &str, intentos: i16) -> CorreoPendiente {
        CorreoPendiente {
            id,
            destinatario: destinatario.into(),
            plantilla: Some(PlantillaCorreo::RecuperacionContrasena {
                nombre: "Laura".into(),
                enlace: "https://x/y".into(),
            }),
            intentos,
        }
    }

    #[tokio::test]
    async fn envia_reintenta_y_se_rinde_segun_los_intentos() {
        let correos = Arc::new(CorreosEnMemoria::default());
        correos.pendientes.lock().unwrap().extend([
            pendiente(1, "laura@diamante.co", 1),
            pendiente(2, "falla@diamante.co", 1),
            pendiente(3, "falla@diamante.co", 5),
            CorreoPendiente {
                plantilla: None,
                ..pendiente(4, "x@diamante.co", 1)
            },
            pendiente(5, "noexiste@diamante.co", 1),
        ]);
        let transporte = Arc::new(TransporteDePrueba::default());
        let reloj = Arc::new(RelojControlado::en("2026-11-20T12:00:00Z".parse().unwrap()));
        let caso = EnviarCorreosPendientes::nuevo(correos.clone(), transporte.clone(), reloj);

        let resumen = caso.ejecutar().await.unwrap();

        assert_eq!(
            resumen,
            ResumenEnvio {
                enviados: 1,
                reintentos: 1,
                fallidos: 3
            }
        );
        assert_eq!(*correos.enviados.lock().unwrap(), [1]);
        let fallos = correos.fallos.lock().unwrap();
        assert_eq!(fallos[0], (2, "2026-11-20T12:01:00Z".parse().ok()));
        assert_eq!(fallos[1], (3, None));
        assert_eq!(fallos[2], (4, None));
        assert_eq!(fallos[3], (5, None));
        assert_eq!(transporte.0.lock().unwrap().len(), 1);
    }
}
