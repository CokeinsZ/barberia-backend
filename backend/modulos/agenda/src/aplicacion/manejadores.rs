//! Reacciones de la agenda a eventos de otros módulos, dentro de la transacción de quien publica.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use contratos::barberos::{AusenciaRegistrada, BloqueoCreado, HorarioCambiado};
use contratos::notificaciones::RespuestaWhatsAppRecibida;
use contratos::ventas::VentaRegistrada;
use notificaciones::api::{NuevoAviso, TipoAviso};
use nucleo::eventos::{ErrorDeManejador, ManejadorEvento};
use nucleo::identificadores::IdBarbero;
use serde_json::json;

use super::errores::ErrorAgenda;
use super::gestion::{cancelar_por_cliente, evento_cliente};
use super::panel::{Propuesta, citas_afectadas};
use super::puertos::{EventoCita, Puertos, fecha_y_hora};
use crate::dominio::cita::EstadoCita;
use crate::dominio::disponibilidad::Franja;

/// Botones del recordatorio (RF-NOT-03): «Confirmo» confirma una cita «Sin confirmar» (RN-31);
/// «Cancelar cita» la cancela como si el cliente usara su enlace. Si la cita ya cambió (ya
/// confirmada, cancelada o pasada), la respuesta no hace nada.
pub(crate) struct AlResponderWhatsApp<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

#[async_trait]
impl<C: Send + 'static> ManejadorEvento<RespuestaWhatsAppRecibida, C> for AlResponderWhatsApp<C> {
    fn nombre(&self) -> &'static str {
        "agenda.al_responder_whatsapp"
    }

    async fn manejar(&self, cx: &mut C, evento: &RespuestaWhatsAppRecibida) -> Result<(), ErrorDeManejador> {
        self.atender(cx, evento).await.map_err(ErrorDeManejador::nuevo)
    }
}

impl<C: Send + 'static> AlResponderWhatsApp<C> {
    async fn atender(&self, cx: &mut C, evento: &RespuestaWhatsAppRecibida) -> Result<(), ErrorAgenda> {
        let p = &self.p;
        let Some((id, proposito)) = p.citas.cita_por_mensaje(cx, evento.mensaje).await? else {
            return Ok(());
        };
        if !proposito.starts_with("recordatorio") {
            return Ok(());
        }
        let cita = p.cita(cx, id, true).await?;
        let ahora = p.reloj.ahora();
        match evento.carga_boton.as_str() {
            "confirmar" if cita.estado == EstadoCita::SinConfirmar && cita.inicio > ahora => {
                p.citas.cambiar_estado(cx, id, EstadoCita::Confirmada).await?;
                let mut historial = evento_cliente(&cita, "confirmada", false, ahora);
                historial.detalle = Some("Confirmó con el botón del recordatorio".into());
                p.citas.registrar_evento(cx, &historial).await?;
            }
            "cancelar"
                if matches!(cita.estado, EstadoCita::SinConfirmar | EstadoCita::Confirmada)
                    && cita.inicio > ahora =>
            {
                cancelar_por_cliente(p, cx, &cita, Some("Canceló con el botón del recordatorio")).await?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Cuando un barbero cambia su horario, se bloquea o se ausenta y eso deja citas por fuera, la
/// administradora recibe un aviso para reasignarlas o pedir a los clientes otra hora
/// (RF-BAR-14, RF-PBA-09, DA-08).
pub(crate) struct AlCambiarDisponibilidad<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send + 'static> AlCambiarDisponibilidad<C> {
    async fn avisar_si_hay_citas(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        propuesta: Propuesta,
        tipo: TipoAviso,
        causa: &str,
    ) -> Result<(), ErrorAgenda> {
        let p = &self.p;
        let afectadas = citas_afectadas(p, cx, barbero, &propuesta).await?;
        if afectadas.is_empty() {
            return Ok(());
        }
        let nombre = p
            .equipo
            .nombres(cx, &[barbero])
            .await?
            .remove(&barbero)
            .unwrap_or_else(|| "un barbero".to_owned());
        let mut ids_clientes: Vec<_> = afectadas.iter().map(|c| c.cliente).collect();
        ids_clientes.sort_by_key(|id| id.0);
        ids_clientes.dedup();
        let clientes = p.clientes.por_ids(cx, &ids_clientes).await?;
        let lineas: Vec<String> = afectadas
            .iter()
            .map(|cita| {
                let (fecha, hora) = fecha_y_hora(cita.inicio);
                let cliente = clientes
                    .iter()
                    .find(|c| c.id == cita.cliente)
                    .map_or("Cliente", |c| c.nombre.as_str());
                format!("{fecha} a las {hora}: {cliente}")
            })
            .collect();
        let cantidad = afectadas.len();
        let titulo = if cantidad == 1 {
            format!("1 cita afectada por {causa} de {nombre}")
        } else {
            format!("{cantidad} citas afectadas por {causa} de {nombre}")
        };
        p.avisos
            .crear(
                cx,
                NuevoAviso {
                    tipo,
                    titulo,
                    detalle: Some(format!(
                        "{}. Reasígnalas a otro barbero o pide a los clientes que elijan otra hora.",
                        lineas.join("; ")
                    )),
                    datos: json!({
                        "barbero_id": barbero,
                        "cita_ids": afectadas.iter().map(|c| c.id).collect::<Vec<_>>(),
                    }),
                },
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl<C: Send + 'static> ManejadorEvento<HorarioCambiado, C> for AlCambiarDisponibilidad<C> {
    fn nombre(&self) -> &'static str {
        "agenda.al_cambiar_horario"
    }

    async fn manejar(&self, cx: &mut C, evento: &HorarioCambiado) -> Result<(), ErrorDeManejador> {
        let hoy = self.p.reloj.hoy();
        let equipo = self
            .p
            .equipo
            .equipo(cx, hoy, hoy)
            .await
            .map_err(ErrorDeManejador::nuevo)?;
        let Some(barbero) = equipo.into_iter().find(|b| b.id == evento.barbero) else {
            return Ok(());
        };
        let franjas = barbero
            .franjas
            .iter()
            .map(|f| Franja {
                dia_semana: f.dia_semana,
                inicio: f.inicio,
                fin: f.fin,
            })
            .collect();
        self.avisar_si_hay_citas(
            cx,
            evento.barbero,
            Propuesta::Horario(franjas),
            TipoAviso::CambioHorarioConCitas,
            "el cambio de horario",
        )
        .await
        .map_err(ErrorDeManejador::nuevo)
    }
}

#[async_trait]
impl<C: Send + 'static> ManejadorEvento<BloqueoCreado, C> for AlCambiarDisponibilidad<C> {
    fn nombre(&self) -> &'static str {
        "agenda.al_crear_bloqueo"
    }

    async fn manejar(&self, cx: &mut C, evento: &BloqueoCreado) -> Result<(), ErrorDeManejador> {
        let propuesta = Propuesta::Bloqueo {
            inicio: evento.inicio.with_timezone(&Utc),
            fin: evento.fin.with_timezone(&Utc),
        };
        self.avisar_si_hay_citas(
            cx,
            evento.barbero,
            propuesta,
            TipoAviso::BloqueoConCitas,
            "el bloqueo",
        )
        .await
        .map_err(ErrorDeManejador::nuevo)
    }
}

#[async_trait]
impl<C: Send + 'static> ManejadorEvento<AusenciaRegistrada, C> for AlCambiarDisponibilidad<C> {
    fn nombre(&self) -> &'static str {
        "agenda.al_registrar_ausencia"
    }

    async fn manejar(&self, cx: &mut C, evento: &AusenciaRegistrada) -> Result<(), ErrorDeManejador> {
        let propuesta = Propuesta::Ausencia {
            desde: evento.desde,
            hasta: evento.hasta,
        };
        self.avisar_si_hay_citas(
            cx,
            evento.barbero,
            propuesta,
            TipoAviso::AusenciaConCitas,
            "la ausencia",
        )
        .await
        .map_err(ErrorDeManejador::nuevo)
    }
}

/// Registrar la venta de una cita la deja «Atendida» (RN-16, RF-VEN-09), en la misma
/// transacción de la venta: si la cita ya no está por atender, la venta tampoco queda.
pub(crate) struct AlRegistrarVenta<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

#[async_trait]
impl<C: Send + 'static> ManejadorEvento<VentaRegistrada, C> for AlRegistrarVenta<C> {
    fn nombre(&self) -> &'static str {
        "agenda.al_registrar_venta"
    }

    async fn manejar(&self, cx: &mut C, evento: &VentaRegistrada) -> Result<(), ErrorDeManejador> {
        let Some(id) = evento.cita else {
            return Ok(());
        };
        let p = &self.p;
        let cita = p.cita(cx, id, true).await.map_err(ErrorDeManejador::nuevo)?;
        if !matches!(
            cita.estado,
            EstadoCita::SinConfirmar | EstadoCita::Confirmada | EstadoCita::EnCurso
        ) {
            return Err(ErrorDeManejador::nuevo("La cita ya no está por atender."));
        }
        p.citas
            .cambiar_estado(cx, id, EstadoCita::Atendida)
            .await
            .map_err(ErrorDeManejador::nuevo)?;
        let historial = EventoCita {
            cita: id,
            evento: "atendida",
            actor: evento.actor,
            usuario: Some(evento.registrada_por),
            inicio_previo: None,
            barbero_previo: None,
            tardio: false,
            detalle: Some(format!("Venta registrada (id {})", evento.venta.0)),
            ahora: p.reloj.ahora(),
        };
        p.citas
            .registrar_evento(cx, &historial)
            .await
            .map_err(ErrorDeManejador::nuevo)
    }
}
