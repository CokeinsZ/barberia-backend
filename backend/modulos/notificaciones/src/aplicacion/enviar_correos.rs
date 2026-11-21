use std::sync::Arc;

use chrono::TimeDelta;
use nucleo::{ErrorPersistencia, Reloj};
use serde_json::json;

use super::puertos::{Canal, RepositorioMensajes, TransporteCorreo};
use crate::api::{PlantillaCorreo, ResumenEnvio};
use crate::dominio::plantillas::renderizar;
use crate::dominio::reintentos::proximo_intento;

pub(crate) const TAMANO_LOTE: i64 = 20;
pub(crate) const RESERVA: TimeDelta = TimeDelta::minutes(5);

/// Una pasada del worker: reserva un lote de correos pendientes y los envía.
#[derive(Debug)]
pub(crate) struct EnviarCorreosPendientes {
    mensajes: Arc<dyn RepositorioMensajes>,
    transporte: Arc<dyn TransporteCorreo>,
    reloj: Arc<dyn Reloj>,
}

impl EnviarCorreosPendientes {
    pub(crate) fn nuevo(
        mensajes: Arc<dyn RepositorioMensajes>,
        transporte: Arc<dyn TransporteCorreo>,
        reloj: Arc<dyn Reloj>,
    ) -> Self {
        Self {
            mensajes,
            transporte,
            reloj,
        }
    }

    pub(crate) async fn ejecutar(&self) -> Result<ResumenEnvio, ErrorPersistencia> {
        let ahora = self.reloj.ahora();
        let lote = self
            .mensajes
            .reservar(Canal::Correo, ahora, ahora + RESERVA, TAMANO_LOTE)
            .await?;
        let mut resumen = ResumenEnvio::default();
        for correo in lote {
            let Ok(plantilla) = serde_json::from_value::<PlantillaCorreo>(
                json!({ "plantilla": correo.plantilla, "variables": correo.variables }),
            ) else {
                tracing::error!(mensaje = correo.id, "mensaje con plantilla desconocida");
                self.mensajes
                    .marcar_fallido(
                        correo.id,
                        self.reloj.ahora(),
                        "plantilla desconocida",
                        false,
                        None,
                    )
                    .await?;
                resumen.fallidos += 1;
                continue;
            };
            match self
                .transporte
                .enviar(&correo.destinatario, &renderizar(&plantilla))
                .await
            {
                Ok(()) => {
                    self.mensajes
                        .marcar_enviado(correo.id, self.reloj.ahora(), None, false)
                        .await?;
                    resumen.enviados += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        mensaje = correo.id,
                        intentos = correo.intentos,
                        permanente = error.permanente,
                        error = error.mensaje,
                        "no se pudo enviar el correo"
                    );
                    let reintento = if error.permanente {
                        None
                    } else {
                        proximo_intento(correo.intentos, self.reloj.ahora())
                    };
                    match reintento {
                        Some(cuando) => {
                            self.mensajes
                                .reintentar(correo.id, &error.mensaje, cuando)
                                .await?;
                            resumen.reintentos += 1;
                        }
                        None => {
                            self.mensajes
                                .marcar_fallido(correo.id, self.reloj.ahora(), &error.mensaje, false, None)
                                .await?;
                            resumen.fallidos += 1;
                        }
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
    use nucleo::RelojControlado;

    use super::*;
    use crate::aplicacion::pruebas::{MensajesEnMemoria, pendiente};
    use crate::aplicacion::puertos::ErrorTransporte;
    use crate::dominio::plantillas::CorreoRenderizado;

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

    fn correo(id: i64, destinatario: &str, intentos: i16) -> crate::aplicacion::puertos::MensajePendiente {
        pendiente(
            id,
            destinatario,
            &PlantillaCorreo::RecuperacionContrasena {
                nombre: "Laura".into(),
                enlace: "https://x/y".into(),
            },
            intentos,
        )
    }

    #[tokio::test]
    async fn envia_reintenta_y_se_rinde_segun_los_intentos() {
        let mensajes = Arc::new(MensajesEnMemoria::default());
        let mut desconocida = correo(4, "x@diamante.co", 1);
        desconocida.plantilla = "no_existe".into();
        mensajes.pendientes.lock().unwrap().extend([
            correo(1, "laura@diamante.co", 1),
            correo(2, "falla@diamante.co", 1),
            correo(3, "falla@diamante.co", 5),
            desconocida,
            correo(5, "noexiste@diamante.co", 1),
        ]);
        let transporte = Arc::new(TransporteDePrueba::default());
        let reloj = Arc::new(RelojControlado::en("2026-11-20T12:00:00Z".parse().unwrap()));
        let caso = EnviarCorreosPendientes::nuevo(mensajes.clone(), transporte.clone(), reloj);

        let resumen = caso.ejecutar().await.unwrap();

        assert_eq!(
            resumen,
            ResumenEnvio {
                enviados: 1,
                reintentos: 1,
                fallidos: 3
            }
        );
        assert_eq!(
            mensajes
                .enviados
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.0)
                .collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            *mensajes.reintentos.lock().unwrap(),
            [(2, "2026-11-20T12:01:00Z".parse().unwrap())]
        );
        let fallidos: Vec<i64> = mensajes.fallidos.lock().unwrap().iter().map(|f| f.0).collect();
        assert_eq!(fallidos, [3, 4, 5]);
        assert_eq!(transporte.0.lock().unwrap().len(), 1);
    }
}
