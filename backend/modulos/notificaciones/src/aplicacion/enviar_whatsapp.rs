use std::sync::Arc;

use nucleo::{ErrorPersistencia, Reloj};
use serde_json::json;

use super::enviar_correos::{RESERVA, TAMANO_LOTE};
use super::puertos::{Canal, PasarelaWhatsApp, RepositorioMensajes};
use crate::api::{
    NOMBRE_POR_DEFECTO, NombreDelNegocio, NuevoAviso, PlantillaWhatsApp, ResumenEnvio, TipoAviso,
};
use crate::dominio::reintentos::proximo_intento;
use crate::dominio::whatsapp::{aviso_de_falla, renderizar, vigencia};

/// Una pasada del worker por los WhatsApp pendientes (RF-NOT-01…05). Si el proveedor falla, el
/// mensaje se reintenta con espera creciente; al agotar los intentos queda fallido y, si es de
/// una cita, la administradora recibe un aviso (RNF-DIS-03, RF-NOT-07).
#[derive(Debug)]
pub(crate) struct EnviarWhatsAppPendientes {
    mensajes: Arc<dyn RepositorioMensajes>,
    pasarela: Arc<dyn PasarelaWhatsApp>,
    negocio: Arc<dyn NombreDelNegocio>,
    reloj: Arc<dyn Reloj>,
}

impl EnviarWhatsAppPendientes {
    pub(crate) fn nuevo(
        mensajes: Arc<dyn RepositorioMensajes>,
        pasarela: Arc<dyn PasarelaWhatsApp>,
        negocio: Arc<dyn NombreDelNegocio>,
        reloj: Arc<dyn Reloj>,
    ) -> Self {
        Self {
            mensajes,
            pasarela,
            negocio,
            reloj,
        }
    }

    pub(crate) async fn ejecutar(&self) -> Result<ResumenEnvio, ErrorPersistencia> {
        let ahora = self.reloj.ahora();
        let lote = self
            .mensajes
            .reservar(Canal::WhatsApp, ahora, ahora + RESERVA, TAMANO_LOTE)
            .await?;
        let mut resumen = ResumenEnvio::default();
        if lote.is_empty() {
            return Ok(resumen);
        }
        let negocio = match self.negocio.nombre().await {
            Some(nombre) if !nombre.trim().is_empty() => nombre,
            _ => {
                tracing::warn!("no se pudo leer el nombre del negocio: se usa el de siempre");
                NOMBRE_POR_DEFECTO.to_owned()
            }
        };
        for mensaje in lote {
            let conservar = self.pasarela.conserva_variables(&mensaje.destinatario);
            let Ok(plantilla) = serde_json::from_value::<PlantillaWhatsApp>(
                json!({ "plantilla": mensaje.plantilla, "variables": mensaje.variables }),
            ) else {
                tracing::error!(mensaje = mensaje.id, "WhatsApp con plantilla desconocida");
                self.mensajes
                    .marcar_fallido(
                        mensaje.id,
                        self.reloj.ahora(),
                        "plantilla desconocida",
                        conservar,
                        None,
                    )
                    .await?;
                resumen.fallidos += 1;
                continue;
            };
            if ahora - mensaje.creado_en > vigencia(&plantilla) {
                let motivo = format!(
                    "No se envió: esperó más de {} en la cola",
                    if vigencia(&plantilla) < chrono::TimeDelta::hours(1) {
                        "10 minutos"
                    } else {
                        "24 horas"
                    }
                );
                tracing::warn!(
                    mensaje = mensaje.id,
                    plantilla = mensaje.plantilla,
                    "WhatsApp vencido en la cola: no se envía"
                );
                self.mensajes
                    .marcar_vencido(mensaje.id, &motivo, conservar)
                    .await?;
                resumen.vencidos += 1;
                continue;
            }
            let renderizado = renderizar(&plantilla, &negocio);
            match self.pasarela.enviar(&mensaje.destinatario, &renderizado).await {
                Ok(id_proveedor) => {
                    self.mensajes
                        .marcar_enviado(mensaje.id, self.reloj.ahora(), Some(&id_proveedor), conservar)
                        .await?;
                    resumen.enviados += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        mensaje = mensaje.id,
                        plantilla = renderizado.plantilla,
                        intentos = mensaje.intentos,
                        permanente = error.permanente,
                        error = error.mensaje,
                        "no se pudo enviar el WhatsApp"
                    );
                    let reintento = if error.permanente {
                        None
                    } else {
                        proximo_intento(mensaje.intentos, self.reloj.ahora())
                    };
                    if let Some(cuando) = reintento {
                        self.mensajes
                            .reintentar(mensaje.id, &error.mensaje, cuando)
                            .await?;
                        resumen.reintentos += 1;
                        continue;
                    }
                    let aviso = aviso_de_falla(&plantilla, &mensaje.destinatario, &error.mensaje).map(
                        |(titulo, detalle)| NuevoAviso {
                            tipo: TipoAviso::MensajeFallido,
                            titulo,
                            detalle: Some(detalle),
                            datos: json!({
                                "mensaje_id": mensaje.id,
                                "plantilla": renderizado.plantilla,
                                "destinatario": mensaje.destinatario,
                            }),
                        },
                    );
                    self.mensajes
                        .marcar_fallido(mensaje.id, self.reloj.ahora(), &error.mensaje, conservar, aviso)
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
    use async_trait::async_trait;
    use nucleo::RelojControlado;

    use super::*;
    use crate::aplicacion::pruebas::{MensajesEnMemoria, pendiente};
    use crate::infraestructura::whatsapp_simulado::WhatsAppSimulado;

    fn recordatorio(id: i64, celular: &str, intentos: i16) -> crate::aplicacion::puertos::MensajePendiente {
        pendiente(
            id,
            celular,
            &PlantillaWhatsApp::RecordatorioCita {
                nombre: "Andrea".into(),
                servicio: "Corte clásico".into(),
                barbero: "Daniel".into(),
                fecha: "lunes 23 de noviembre".into(),
                hora: "9:00 a. m.".into(),
            },
            intentos,
        )
    }

    #[tokio::test]
    async fn envia_reintenta_y_avisa_al_agotar_los_intentos() {
        let mensajes = Arc::new(MensajesEnMemoria::default());
        let codigo = pendiente(
            5,
            "3001119999",
            &PlantillaWhatsApp::CodigoVerificacion {
                codigo: "4821".into(),
            },
            1,
        );
        mensajes.pendientes.lock().unwrap().extend([
            recordatorio(1, "3001112233", 1),
            recordatorio(2, "3001110000", 1),
            recordatorio(3, "3001110000", 5),
            recordatorio(4, "3001119999", 1),
            codigo,
        ]);
        let reloj = Arc::new(RelojControlado::en("2026-11-21T15:00:00Z".parse().unwrap()));
        let caso = EnviarWhatsAppPendientes::nuevo(
            mensajes.clone(),
            Arc::new(WhatsAppSimulado),
            Arc::new(NegocioFijo(None)),
            reloj,
        );

        let resumen = caso.ejecutar().await.unwrap();

        assert_eq!(
            resumen,
            ResumenEnvio {
                enviados: 1,
                reintentos: 1,
                fallidos: 3,
                vencidos: 0,
            }
        );
        let enviados = mensajes.enviados.lock().unwrap();
        assert_eq!(enviados[0].0, 1);
        assert!(enviados[0].1.as_deref().unwrap().starts_with("wamid.simulado."));
        assert!(enviados[0].2, "el simulador conserva las variables");
        assert_eq!(
            *mensajes.reintentos.lock().unwrap(),
            [(2, "2026-11-21T15:01:00Z".parse().unwrap())]
        );
        let fallidos = mensajes.fallidos.lock().unwrap();
        let resumen: Vec<(i64, bool)> = fallidos.iter().map(|f| (f.0, f.2.is_some())).collect();
        assert_eq!(
            resumen,
            [(3, true), (4, true), (5, false)],
            "aviso solo para los mensajes de citas"
        );
        let aviso = fallidos[0].2.as_ref().unwrap();
        assert_eq!(aviso.tipo, TipoAviso::MensajeFallido);
        assert_eq!(aviso.datos["mensaje_id"], 3);
    }

    #[tokio::test]
    async fn no_envia_lo_que_vencio_en_la_cola() {
        use crate::aplicacion::pruebas::ENCOLADO;
        let mensajes = Arc::new(MensajesEnMemoria::default());
        let codigo = |id| {
            pendiente(
                id,
                "3001112233",
                &PlantillaWhatsApp::CodigoVerificacion {
                    codigo: "4821".into(),
                },
                1,
            )
        };
        mensajes
            .pendientes
            .lock()
            .unwrap()
            .extend([recordatorio(1, "3001112233", 1), codigo(2)]);
        let encolado: chrono::DateTime<chrono::Utc> = ENCOLADO.parse().unwrap();
        let caso = |reloj| {
            EnviarWhatsAppPendientes::nuevo(
                mensajes.clone(),
                Arc::new(WhatsAppSimulado),
                Arc::new(NegocioFijo(None)),
                Arc::new(RelojControlado::en(reloj)),
            )
        };
        // A los 11 minutos: el código ya no sirve; el recordatorio sí sale.
        let resumen = caso(encolado + chrono::TimeDelta::minutes(11))
            .ejecutar()
            .await
            .unwrap();
        assert_eq!((resumen.enviados, resumen.vencidos), (1, 1));
        assert_eq!(mensajes.vencidos.lock().unwrap()[0].0, 2);
        assert!(mensajes.vencidos.lock().unwrap()[0].1.contains("10 minutos"));
        // Un día y una hora después, ni el recordatorio.
        mensajes
            .pendientes
            .lock()
            .unwrap()
            .push(recordatorio(3, "3001112233", 1));
        let resumen = caso(encolado + chrono::TimeDelta::hours(25))
            .ejecutar()
            .await
            .unwrap();
        assert_eq!((resumen.enviados, resumen.vencidos), (0, 1));
        assert!(mensajes.vencidos.lock().unwrap()[1].1.contains("24 horas"));
    }

    /// Pasarela que recuerda lo enviado, para ver el texto renderizado.
    #[derive(Debug, Default)]
    struct PasarelaQueAnota(std::sync::Mutex<Vec<String>>);

    #[async_trait]
    impl PasarelaWhatsApp for PasarelaQueAnota {
        async fn enviar(
            &self,
            celular: &str,
            mensaje: &crate::dominio::whatsapp::WhatsAppRenderizado,
        ) -> Result<String, crate::aplicacion::puertos::ErrorTransporte> {
            self.0
                .lock()
                .unwrap()
                .push(format!("{celular}: {}", mensaje.texto));
            Ok("wamid.1".into())
        }
    }

    #[tokio::test]
    async fn con_un_proveedor_real_las_variables_secretas_no_se_conservan() {
        let mensajes = Arc::new(MensajesEnMemoria::default());
        mensajes
            .pendientes
            .lock()
            .unwrap()
            .push(recordatorio(1, "3001112233", 1));
        let pasarela = Arc::new(PasarelaQueAnota::default());
        let reloj = Arc::new(RelojControlado::en("2026-11-21T15:00:00Z".parse().unwrap()));
        EnviarWhatsAppPendientes::nuevo(
            mensajes.clone(),
            pasarela.clone(),
            Arc::new(NegocioFijo(Some("Barbería del Centro".into()))),
            reloj,
        )
        .ejecutar()
        .await
        .unwrap();
        assert!(!mensajes.enviados.lock().unwrap()[0].2);
        assert!(
            pasarela.0.lock().unwrap()[0]
                .starts_with("3001112233: Hola Andrea.\nTe recordamos tu cita en Barbería del Centro el"),
            "el nombre del negocio va en la plantilla"
        );
    }

    /// Nombre del negocio fijo; `None` simula que no se pudo leer.
    #[derive(Debug)]
    struct NegocioFijo(Option<String>);

    #[async_trait]
    impl NombreDelNegocio for NegocioFijo {
        async fn nombre(&self) -> Option<String> {
            self.0.clone()
        }
    }
}
