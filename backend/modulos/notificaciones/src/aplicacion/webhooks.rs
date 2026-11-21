use std::sync::Arc;

use contratos::notificaciones::RespuestaWhatsAppRecibida;
use nucleo::eventos::BusEventos;
use nucleo::{ErrorPersistencia, Reloj, Transacciones};
use serde_json::json;

use super::puertos::{EventoWebhook, RepositorioWebhooks};
use crate::api::{Avisos, NuevoAviso, PlantillaWhatsApp, ResumenWebhooks, TipoAviso};
use crate::dominio::webhook::{EstadoEntrega, interpretar, puede_pasar};
use crate::dominio::whatsapp::aviso_de_falla;

/// Eventos por pasada: los webhooks llegan de a pocos y el resto queda para la siguiente.
const LOTE: usize = 50;

/// Procesa los webhooks guardados (el endpoint solo los guarda y responde `200` de inmediato).
/// Cada evento va en su propia transacción: el cambio de estado del mensaje, el aviso y la
/// reacción de la agenda a una respuesta («Confirmo», «Cancelar cita») quedan juntos o no quedan.
pub(crate) struct ProcesarWebhooks<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) webhooks: Arc<dyn RepositorioWebhooks<C>>,
    pub(crate) avisos: Arc<dyn Avisos<C>>,
    pub(crate) bus: Arc<BusEventos<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> std::fmt::Debug for ProcesarWebhooks<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcesarWebhooks")
    }
}

#[derive(Debug, Default)]
struct Parcial {
    estados: usize,
    respuestas: usize,
}

impl<C: Send + 'static> ProcesarWebhooks<C> {
    pub(crate) async fn ejecutar(&self) -> Result<ResumenWebhooks, ErrorPersistencia> {
        let mut resumen = ResumenWebhooks::default();
        for _ in 0..LOTE {
            let mut cx = self.transacciones.iniciar().await?;
            let Some(evento) = self.webhooks.tomar(&mut cx).await? else {
                break;
            };
            resumen.eventos += 1;
            match self.procesar(&mut cx, &evento).await {
                Ok(parcial) => {
                    self.webhooks
                        .marcar_procesado(&mut cx, evento.id, self.reloj.ahora(), None)
                        .await?;
                    self.transacciones.confirmar(cx).await?;
                    resumen.estados += parcial.estados;
                    resumen.respuestas += parcial.respuestas;
                }
                Err(error) => {
                    // Se revierte lo hecho y el evento queda marcado con el error: no se
                    // reintenta en bucle (se puede revisar en `evento_webhook`).
                    drop(cx);
                    tracing::warn!(evento = evento.id, error, "no se pudo procesar el webhook");
                    let mut cx = self.transacciones.iniciar().await?;
                    self.webhooks
                        .marcar_procesado(&mut cx, evento.id, self.reloj.ahora(), Some(&error))
                        .await?;
                    self.transacciones.confirmar(cx).await?;
                    resumen.con_error += 1;
                }
            }
        }
        Ok(resumen)
    }

    async fn procesar(&self, cx: &mut C, evento: &EventoWebhook) -> Result<Parcial, String> {
        let interpretacion = interpretar(&evento.carga);
        let ahora = self.reloj.ahora();
        let mut parcial = Parcial::default();
        for cambio in interpretacion.estados {
            let Some(mensaje) = self
                .webhooks
                .mensaje_por_proveedor(cx, &cambio.id_proveedor)
                .await
                .map_err(|e| e.to_string())?
            else {
                continue;
            };
            if !puede_pasar(&mensaje.estado, cambio.estado) {
                continue;
            }
            self.webhooks
                .cambiar_estado(cx, mensaje.id, cambio.estado, cambio.error.as_deref(), ahora)
                .await
                .map_err(|e| e.to_string())?;
            parcial.estados += 1;
            if cambio.estado == EstadoEntrega::Fallido {
                let plantilla = serde_json::from_value::<PlantillaWhatsApp>(
                    json!({ "plantilla": mensaje.plantilla, "variables": mensaje.variables }),
                );
                let motivo = cambio.error.as_deref().unwrap_or("Meta no lo pudo entregar");
                if let Some((titulo, detalle)) = plantilla
                    .ok()
                    .and_then(|p| aviso_de_falla(&p, &mensaje.destinatario, motivo))
                {
                    let aviso = NuevoAviso {
                        tipo: TipoAviso::MensajeFallido,
                        titulo,
                        detalle: Some(detalle),
                        datos: json!({
                            "mensaje_id": mensaje.id,
                            "plantilla": mensaje.plantilla,
                            "destinatario": mensaje.destinatario,
                        }),
                    };
                    self.avisos.crear(cx, aviso).await.map_err(|e| e.to_string())?;
                }
            }
        }
        for respuesta in interpretacion.respuestas {
            let Some(mensaje) = self
                .webhooks
                .mensaje_por_proveedor(cx, &respuesta.id_original)
                .await
                .map_err(|e| e.to_string())?
            else {
                tracing::info!(id = respuesta.id_original, "respuesta a un mensaje desconocido");
                continue;
            };
            let evento = RespuestaWhatsAppRecibida {
                mensaje: mensaje.id,
                id_proveedor: respuesta.id_original,
                carga_boton: respuesta.carga.to_owned(),
                recibida_en: ahora,
            };
            self.bus.publicar(cx, &evento).await.map_err(|e| e.to_string())?;
            parcial.respuestas += 1;
        }
        Ok(parcial)
    }
}
