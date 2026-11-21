//! Gestión de la cita desde el enlace del WhatsApp (RF-RES-14, 15 y 18).

use std::sync::Arc;

use chrono::{DateTime, FixedOffset, TimeDelta, Utc};
use contratos::agenda::{CitaCancelada, CitaReprogramada};
use nucleo::identificadores::IdBarbero;
use nucleo::{ErrorPersistencia, TipoActor};
use plataforma::seguridad::huella_token;

use super::dto::GestionCitaDto;
use super::errores::ErrorAgenda;
use super::puertos::{
    CitaGuardada, EventoCita, Puertos, RESTRICCION_SOLAPE, alternativas, enlace_google_calendar,
};
use crate::dominio::cita::cancelacion_tardia;
use crate::dominio::disponibilidad::Modo;

pub(crate) struct Gestion<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Gestion<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gestion")
    }
}

impl<C: Send + 'static> Gestion<C> {
    async fn cita_del_enlace(
        &self,
        cx: &mut C,
        token: &str,
        bloquear: bool,
    ) -> Result<CitaGuardada, ErrorAgenda> {
        let id = self
            .p
            .citas
            .por_enlace(cx, &huella_token(token), self.p.reloj.ahora())
            .await?
            .ok_or(ErrorAgenda::EnlaceVencido)?;
        self.p.cita(cx, id, bloquear).await
    }

    fn evento(cita: &CitaGuardada, evento: &'static str, tardio: bool, ahora: DateTime<Utc>) -> EventoCita {
        EventoCita {
            cita: cita.id,
            evento,
            actor: TipoActor::Cliente,
            usuario: None,
            inicio_previo: None,
            barbero_previo: None,
            tardio,
            detalle: None,
            ahora,
        }
    }

    pub(crate) async fn ver(&self, token: &str) -> Result<GestionCitaDto, ErrorAgenda> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let cita = self.cita_del_enlace(&mut cx, token, false).await?;
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let negocio = p.negocio.negocio(&mut cx).await?;
        let recordatorios_activos = p
            .clientes
            .por_ids(&mut cx, &[cita.cliente])
            .await?
            .first()
            .is_some_and(|c| c.recordatorios_activos);
        let abierta = cita.estado.pendiente() && cita.inicio > ahora;
        let tardia = cancelacion_tardia(
            cita.inicio,
            ahora,
            TimeDelta::minutes(i64::from(configuracion.aviso_cancelacion_min)),
        );
        let (inicio, fin) = (cita.inicio, cita.fin);
        let dto = p.dto(&mut cx, cita, false).await?;
        p.transacciones.confirmar(cx).await?;
        let titulo = format!("{} · {}", dto.servicio.nombre, negocio.nombre_comercial);
        Ok(GestionCitaDto {
            cita: dto,
            calendario_google: enlace_google_calendar(&titulo, inicio, fin, negocio.direccion.as_deref()),
            direccion: negocio.direccion,
            puede_cancelar: abierta,
            puede_reprogramar: abierta,
            cancelacion_seria_tardia: abierta && tardia,
            recordatorios_activos,
        })
    }

    /// Cancela hasta la hora de inicio; dentro del aviso mínimo queda marcada como tardía, pero
    /// nunca como inasistencia (RN-09, DA-17).
    pub(crate) async fn cancelar(&self, token: &str) -> Result<GestionCitaDto, ErrorAgenda> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let cita = self.cita_del_enlace(&mut cx, token, true).await?;
        if !cita.estado.pendiente() || cita.inicio <= ahora {
            return Err(ErrorAgenda::EstadoInvalido("La cita ya no se puede cancelar."));
        }
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let tardia = cancelacion_tardia(
            cita.inicio,
            ahora,
            TimeDelta::minutes(i64::from(configuracion.aviso_cancelacion_min)),
        );
        p.citas
            .cancelar(
                &mut cx,
                cita.id,
                TipoActor::Cliente,
                "solicitud_cliente",
                tardia,
                ahora,
            )
            .await?;
        p.citas
            .registrar_evento(&mut cx, &Self::evento(&cita, "cancelada", tardia, ahora))
            .await?;
        p.avisar(&mut cx, &cita, "cancelacion", None).await?;
        p.publicar(
            &mut cx,
            &CitaCancelada {
                cita: cita.id,
                tardia,
                avisar_cliente: true,
            },
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        self.ver(token).await
    }

    /// Reprograma hasta la hora de inicio, con las mismas reglas de disponibilidad (RF-RES-15).
    pub(crate) async fn reprogramar(
        &self,
        token: &str,
        inicio: DateTime<FixedOffset>,
        barbero: Option<IdBarbero>,
    ) -> Result<GestionCitaDto, ErrorAgenda> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let inicio = inicio.with_timezone(&Utc);
        let mut cx = p.transacciones.iniciar().await?;
        let cita = self.cita_del_enlace(&mut cx, token, true).await?;
        if !cita.estado.pendiente() || cita.inicio <= ahora {
            return Err(ErrorAgenda::EstadoInvalido("La cita ya no se puede reprogramar."));
        }
        let servicio = p.servicio(&mut cx, cita.servicio, Modo::Publico).await?;
        let (nuevo_barbero, libres) = p
            .reservar_hora(&mut cx, &servicio, barbero, inicio, Modo::Publico, Some(cita.id))
            .await?;
        let fin = inicio + (cita.fin - cita.inicio);
        match p
            .citas
            .mover(&mut cx, cita.id, nuevo_barbero, inicio, fin, barbero.is_none())
            .await
        {
            Ok(()) => {}
            Err(ErrorPersistencia::Conflicto { restriccion }) if restriccion == RESTRICCION_SOLAPE => {
                return Err(ErrorAgenda::ConflictoHorario {
                    alternativas: alternativas(&libres, inicio),
                });
            }
            Err(otro) => return Err(otro.into()),
        }
        let mut evento = Self::evento(&cita, "reprogramada", false, ahora);
        evento.inicio_previo = Some(cita.inicio);
        evento.barbero_previo = Some(cita.barbero);
        p.citas.registrar_evento(&mut cx, &evento).await?;
        let movida = CitaGuardada {
            barbero: nuevo_barbero,
            inicio,
            fin,
            ..cita.clone()
        };
        p.avisar(&mut cx, &movida, "reprogramacion", Some(p.enlace_gestion(token)))
            .await?;
        p.publicar(
            &mut cx,
            &CitaReprogramada {
                cita: cita.id,
                inicio_anterior: cita.inicio,
                inicio_nuevo: inicio,
                barbero_nuevo: nuevo_barbero,
            },
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        self.ver(token).await
    }

    /// Activa o desactiva los recordatorios por WhatsApp del cliente (RF-RES-18).
    pub(crate) async fn recordatorios(
        &self,
        token: &str,
        activos: bool,
    ) -> Result<GestionCitaDto, ErrorAgenda> {
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let cita = self.cita_del_enlace(&mut cx, token, false).await?;
        p.clientes
            .cambiar_recordatorios(&mut cx, cita.cliente, activos)
            .await?;
        p.transacciones.confirmar(cx).await?;
        self.ver(token).await
    }
}
