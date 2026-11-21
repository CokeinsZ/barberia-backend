//! Agenda del personal (RF-AGE-01 a 08, 10 y 12, RF-BAR-13, RF-CFG-02).

use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime, TimeDelta, Utc};
use clientes::api::NuevoCliente;
use contratos::agenda::{CitaCancelada, CitaCreada, CitaReprogramada, InasistenciaMarcada};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio};
use nucleo::{Actor, ErrorPersistencia, ZONA_HORARIA};
use plataforma::seguridad::generar_token;

use super::dto::{CitaDto, ColumnaTableroDto, DetalleCitaDto, RangoHorasDto, RangoInstantesDto, TableroDto};
use super::errores::ErrorAgenda;
use super::puertos::{
    CitaGuardada, EventoCita, FiltroCitas, NuevaCita, Puertos, RESTRICCION_SOLAPE, a_agendable, alternativas,
    inicio_del_dia, local,
};
use super::reserva::VIGENCIA_ENLACE_DESPUES;
use crate::dominio::cita::{EstadoCita, cancelacion_tardia, puede_marcar_inasistencia};
use crate::dominio::configuracion::ConfiguracionAgenda;
use crate::dominio::disponibilidad::{Franja, Modo, TipoDia, franjas_del_dia};

fn solo_administrador(actor: &Actor) -> Result<(), ErrorAgenda> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorAgenda::Prohibido)
    }
}

/// El administrador, o el barbero sobre sus propias citas (lo ajeno «no existe», 404).
fn administrador_o_propio(actor: &Actor, barbero: IdBarbero) -> Result<(), ErrorAgenda> {
    match actor {
        Actor::Administrador { .. } => Ok(()),
        Actor::Barbero { barbero: propio, .. } if *propio == barbero => Ok(()),
        Actor::Barbero { .. } => Err(ErrorAgenda::CitaNoEncontrada),
        _ => Err(ErrorAgenda::Prohibido),
    }
}

/// Cita creada por el personal (RF-AGE-05).
#[derive(Debug, Clone)]
pub(crate) struct SolicitudCitaPanel {
    pub(crate) cliente_id: Option<IdCliente>,
    pub(crate) cliente_nuevo: Option<NuevoCliente>,
    pub(crate) servicio_id: IdServicio,
    /// `None`: el primero libre según el orden del equipo.
    pub(crate) barbero_id: Option<IdBarbero>,
    pub(crate) inicio: DateTime<FixedOffset>,
    pub(crate) nota: Option<String>,
}

/// Cambio propuesto en la disponibilidad de un barbero, para ver antes de guardar qué citas
/// quedarían afectadas (RF-BAR-13, RF-PBA-09).
#[derive(Debug, Clone)]
pub(crate) enum Propuesta {
    Ausencia {
        desde: NaiveDate,
        hasta: NaiveDate,
    },
    Bloqueo {
        inicio: DateTime<Utc>,
        fin: DateTime<Utc>,
    },
    Horario(Vec<Franja>),
}

pub(crate) struct Panel<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Panel<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Panel")
    }
}

impl<C: Send + 'static> Panel<C> {
    fn evento(&self, cita: IdCita, evento: &'static str, actor: &Actor) -> EventoCita {
        EventoCita {
            cita,
            evento,
            actor: actor.tipo(),
            usuario: actor.usuario(),
            inicio_previo: None,
            barbero_previo: None,
            tardio: false,
            detalle: None,
            ahora: self.p.reloj.ahora(),
        }
    }

    /// Vista de día: una columna por barbero con sus franjas, bloqueos y citas (RF-AGE-01, 03).
    pub(crate) async fn tablero(&self, actor: &Actor, fecha: NaiveDate) -> Result<TableroDto, ErrorAgenda> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let equipo = p.equipo.equipo(&mut cx, fecha, fecha).await?;
        let especial = p
            .calendario
            .dias_especiales(&mut cx, fecha, fecha)
            .await?
            .into_iter()
            .next();
        let (desde, hasta) = (inicio_del_dia(fecha), inicio_del_dia(fecha) + TimeDelta::days(1));
        let citas = p
            .citas
            .listar(
                &mut cx,
                &FiltroCitas {
                    desde: Some(desde),
                    hasta: Some(hasta),
                    ..Default::default()
                },
            )
            .await?;
        let dtos = p.dtos(&mut cx, citas, true).await?;
        p.transacciones.confirmar(cx).await?;
        let (tipo, tipo_dia) = match especial.map(|d| (d.tipo, d.hora_cierre)) {
            Some((negocio::api::TipoDiaEspecialDto::Festivo, _)) => (TipoDia::Festivo, "festivo"),
            Some((negocio::api::TipoDiaEspecialDto::Cerrado, _)) => (TipoDia::Cerrado, "cerrado"),
            Some((negocio::api::TipoDiaEspecialDto::CierreTemprano, Some(h))) => {
                (TipoDia::CierreTemprano(h), "cierre_temprano")
            }
            _ => (TipoDia::Normal, "normal"),
        };
        let mut columnas: Vec<ColumnaTableroDto> = equipo
            .iter()
            .map(|b| {
                let agendable = a_agendable(b, std::iter::empty());
                let ausencia = b.ausencias.iter().find(|a| (a.desde..=a.hasta).contains(&fecha));
                let franjas = franjas_del_dia(&agendable, fecha, tipo);
                let estado = match ausencia {
                    Some(a) => a.tipo.clone(),
                    None if franjas.is_empty() => "no_trabaja".to_owned(),
                    None => "trabaja".to_owned(),
                };
                ColumnaTableroDto {
                    barbero_id: b.id,
                    nombre: b.nombre.clone(),
                    estado,
                    franjas: franjas
                        .into_iter()
                        .map(|(inicio, fin)| RangoHorasDto { inicio, fin })
                        .collect(),
                    bloqueos: b
                        .bloqueos
                        .iter()
                        .filter(|(i, f)| *i < hasta && *f > desde)
                        .map(|(i, f)| RangoInstantesDto {
                            inicio: local(*i),
                            fin: local(*f),
                        })
                        .collect(),
                    citas: dtos.iter().filter(|c| c.barbero.id == b.id).cloned().collect(),
                }
            })
            .collect();
        // Citas de barberos que ya no están en el equipo (retirados): también se muestran.
        for cita in dtos
            .iter()
            .filter(|c| !equipo.iter().any(|b| b.id == c.barbero.id))
        {
            match columnas.iter_mut().find(|col| col.barbero_id == cita.barbero.id) {
                Some(col) => col.citas.push(cita.clone()),
                None => columnas.push(ColumnaTableroDto {
                    barbero_id: cita.barbero.id,
                    nombre: cita.barbero.nombre.clone(),
                    estado: "inactivo".into(),
                    franjas: Vec::new(),
                    bloqueos: Vec::new(),
                    citas: vec![cita.clone()],
                }),
            }
        }
        let vigentes = |c: &&CitaDto| c.estado != "cancelada";
        Ok(TableroDto {
            fecha,
            tipo_dia,
            total_citas: dtos.iter().filter(vigentes).count(),
            por_atender: dtos
                .iter()
                .filter(|c| matches!(c.estado, "sin_confirmar" | "confirmada" | "en_curso"))
                .count(),
            barberos: columnas,
        })
    }

    /// Lista de citas (RF-AGE-02). Un barbero solo ve las suyas (RN-26).
    pub(crate) async fn listar(
        &self,
        actor: &Actor,
        desde: NaiveDate,
        hasta: NaiveDate,
        barbero: Option<IdBarbero>,
        estado: Option<String>,
    ) -> Result<Vec<CitaDto>, ErrorAgenda> {
        let barbero = match actor {
            Actor::Administrador { .. } => barbero,
            Actor::Barbero { barbero: propio, .. } => Some(*propio),
            _ => return Err(ErrorAgenda::Prohibido),
        };
        if hasta < desde || (hasta - desde).num_days() > 62 {
            return Err(ErrorAgenda::campo(
                "hasta",
                "Consulta un rango de máximo dos meses.",
            ));
        }
        let estados = match estado.as_deref().filter(|e| !e.is_empty()) {
            None => Vec::new(),
            Some(codigo) => vec![
                EstadoCita::desde_codigo(codigo)
                    .ok_or_else(|| ErrorAgenda::campo("estado", "Estado desconocido."))?,
            ],
        };
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let filtro = FiltroCitas {
            desde: Some(inicio_del_dia(desde)),
            hasta: Some(inicio_del_dia(hasta) + TimeDelta::days(1)),
            barbero,
            estados,
        };
        let citas = p.citas.listar(&mut cx, &filtro).await?;
        let dtos = p.dtos(&mut cx, citas, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dtos)
    }

    /// Detalle con cliente e historial (RF-AGE-04).
    pub(crate) async fn detalle(&self, actor: &Actor, id: IdCita) -> Result<DetalleCitaDto, ErrorAgenda> {
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let cita = p.cita(&mut cx, id, false).await?;
        administrador_o_propio(actor, cita.barbero)?;
        let historial = p.citas.historial(&mut cx, id).await?;
        let dto = p.dto(&mut cx, cita, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(DetalleCitaDto { cita: dto, historial })
    }

    /// Cita creada por el personal: sin código ni anticipación mínima; queda «Sin confirmar»
    /// (RN-31) y el cliente recibe la confirmación con su enlace.
    pub(crate) async fn crear(
        &self,
        actor: &Actor,
        s: SolicitudCitaPanel,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        solo_administrador(actor)?;
        let nota = s.nota.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        if nota.as_ref().is_some_and(|n| n.chars().count() > 500) {
            return Err(ErrorAgenda::campo(
                "nota",
                "La nota admite máximo 500 caracteres.",
            ));
        }
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let inicio = s.inicio.with_timezone(&Utc);
        let mut cx = p.transacciones.iniciar().await?;
        let cliente = match (s.cliente_id, s.cliente_nuevo) {
            (Some(id), _) => {
                if p.clientes.por_ids(&mut cx, &[id]).await?.is_empty() {
                    return Err(ErrorAgenda::campo("cliente_id", "El cliente no existe."));
                }
                id
            }
            (None, Some(nuevo)) => p.clientes.registrar_desde_panel(&mut cx, actor, nuevo).await?,
            (None, None) => return Err(ErrorAgenda::campo("cliente_id", "Busca o registra al cliente.")),
        };
        let servicio = p.servicio(&mut cx, s.servicio_id, Modo::Personal).await?;
        let (barbero, libres) = p
            .reservar_hora(&mut cx, &servicio, s.barbero_id, inicio, Modo::Personal, None)
            .await?;
        let fin = inicio + TimeDelta::minutes(i64::from(servicio.duracion_min.unwrap_or(30)));
        let nueva = NuevaCita {
            cliente,
            barbero,
            servicio: servicio.id,
            inicio,
            fin,
            precio: servicio.precio.valor(),
            estado: EstadoCita::SinConfirmar,
            creada_por: actor.usuario(),
            asignacion_automatica: s.barbero_id.is_none(),
            nota,
            ahora,
        };
        let id = match p.citas.crear(&mut cx, &nueva).await {
            Ok(id) => id,
            Err(ErrorPersistencia::Conflicto { restriccion }) if restriccion == RESTRICCION_SOLAPE => {
                return Err(ErrorAgenda::ConflictoHorario {
                    alternativas: alternativas(&libres, inicio),
                });
            }
            Err(otro) => return Err(otro.into()),
        };
        let (token, huella) = generar_token().map_err(|e| ErrorAgenda::Interno(e.0))?;
        p.citas
            .guardar_enlace(&mut cx, id, &huella, fin + VIGENCIA_ENLACE_DESPUES)
            .await?;
        p.citas
            .registrar_evento(&mut cx, &self.evento(id, "creada", actor))
            .await?;
        let cita = p.cita(&mut cx, id, false).await?;
        p.avisar(&mut cx, &cita, "confirmacion", Some(p.enlace_gestion(&token)))
            .await?;
        p.publicar(
            &mut cx,
            &CitaCreada {
                cita: id,
                cliente,
                barbero,
                inicio,
            },
        )
        .await?;
        let historial = p.citas.historial(&mut cx, id).await?;
        let dto = p.dto(&mut cx, cita, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(DetalleCitaDto { cita: dto, historial })
    }

    /// Reprograma a otra fecha, hora o barbero, respetando la disponibilidad (RF-AGE-06). El
    /// cliente recibe el aviso con un enlace nuevo (el anterior deja de servir).
    pub(crate) async fn reprogramar(
        &self,
        actor: &Actor,
        id: IdCita,
        inicio: DateTime<FixedOffset>,
        barbero: Option<IdBarbero>,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        solo_administrador(actor)?;
        let p = &self.p;
        let inicio = inicio.with_timezone(&Utc);
        let mut cx = p.transacciones.iniciar().await?;
        let cita = p.cita(&mut cx, id, true).await?;
        if !cita.estado.pendiente() {
            return Err(ErrorAgenda::EstadoInvalido(
                "Solo se reprograman citas sin atender.",
            ));
        }
        let servicio = p.servicio(&mut cx, cita.servicio, Modo::Personal).await?;
        let pedido = barbero.or(Some(cita.barbero));
        let (nuevo_barbero, libres) = p
            .reservar_hora(&mut cx, &servicio, pedido, inicio, Modo::Personal, Some(id))
            .await?;
        let fin = inicio + (cita.fin - cita.inicio);
        match p
            .citas
            .mover(&mut cx, id, nuevo_barbero, inicio, fin, false)
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
        let mut evento = self.evento(
            id,
            if nuevo_barbero == cita.barbero {
                "reprogramada"
            } else {
                "reasignada"
            },
            actor,
        );
        evento.inicio_previo = Some(cita.inicio);
        evento.barbero_previo = Some(cita.barbero);
        p.citas.registrar_evento(&mut cx, &evento).await?;
        let (token, huella) = generar_token().map_err(|e| ErrorAgenda::Interno(e.0))?;
        p.citas
            .guardar_enlace(&mut cx, id, &huella, fin + VIGENCIA_ENLACE_DESPUES)
            .await?;
        let movida = p.cita(&mut cx, id, false).await?;
        p.avisar(&mut cx, &movida, "reprogramacion", Some(p.enlace_gestion(&token)))
            .await?;
        p.publicar(
            &mut cx,
            &CitaReprogramada {
                cita: id,
                inicio_anterior: cita.inicio,
                inicio_nuevo: inicio,
                barbero_nuevo: nuevo_barbero,
            },
        )
        .await?;
        let historial = p.citas.historial(&mut cx, id).await?;
        let dto = p.dto(&mut cx, movida, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(DetalleCitaDto { cita: dto, historial })
    }

    /// Cancela con motivo y, si se pide, avisa al cliente; el horario queda libre (RF-AGE-07).
    pub(crate) async fn cancelar(
        &self,
        actor: &Actor,
        id: IdCita,
        motivo: &str,
        avisar_cliente: bool,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        solo_administrador(actor)?;
        if !["solicitud_cliente", "barbero_no_disponible", "otro"].contains(&motivo) {
            return Err(ErrorAgenda::campo(
                "motivo",
                "Elige solicitud_cliente, barbero_no_disponible u otro.",
            ));
        }
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let cita = p.cita(&mut cx, id, true).await?;
        if !cita.estado.pendiente() {
            return Err(ErrorAgenda::EstadoInvalido("Solo se cancelan citas sin atender."));
        }
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let tardia = motivo == "solicitud_cliente"
            && cancelacion_tardia(
                cita.inicio,
                ahora,
                TimeDelta::minutes(i64::from(configuracion.aviso_cancelacion_min)),
            );
        p.citas
            .cancelar(&mut cx, id, actor.tipo(), motivo, tardia, ahora)
            .await?;
        let mut evento = self.evento(id, "cancelada", actor);
        evento.tardio = tardia;
        evento.detalle = Some(motivo.to_owned());
        p.citas.registrar_evento(&mut cx, &evento).await?;
        if avisar_cliente {
            p.avisar(&mut cx, &cita, "cancelacion", None).await?;
        }
        p.publicar(
            &mut cx,
            &CitaCancelada {
                cita: id,
                tardia,
                avisar_cliente,
            },
        )
        .await?;
        let historial = p.citas.historial(&mut cx, id).await?;
        let cancelada = p.cita(&mut cx, id, false).await?;
        let dto = p.dto(&mut cx, cancelada, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(DetalleCitaDto { cita: dto, historial })
    }

    /// Cambios de estado simples: confirmar (administrador), iniciar e inasistencia
    /// (administrador o el barbero de la cita).
    async fn cambiar_estado(
        &self,
        actor: &Actor,
        id: IdCita,
        nuevo: EstadoCita,
        evento: &'static str,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let cita = p.cita(&mut cx, id, true).await?;
        match nuevo {
            EstadoCita::Confirmada => {
                solo_administrador(actor)?;
                if cita.estado != EstadoCita::SinConfirmar {
                    return Err(ErrorAgenda::EstadoInvalido(
                        "Solo se confirman citas sin confirmar.",
                    ));
                }
            }
            EstadoCita::EnCurso => {
                administrador_o_propio(actor, cita.barbero)?;
                if !cita.estado.pendiente() {
                    return Err(ErrorAgenda::EstadoInvalido("Solo se inician citas pendientes."));
                }
            }
            EstadoCita::NoAsistio => {
                administrador_o_propio(actor, cita.barbero)?;
                let fecha = cita.inicio.with_timezone(&ZONA_HORARIA).date_naive();
                if !puede_marcar_inasistencia(cita.estado, fecha, p.reloj.hoy()) {
                    return Err(ErrorAgenda::EstadoInvalido(
                        "«No asistió» solo se marca el día de la cita o después, en citas sin atender.",
                    ));
                }
            }
            _ => return Err(ErrorAgenda::EstadoInvalido("Cambio de estado no permitido.")),
        }
        p.citas.cambiar_estado(&mut cx, id, nuevo).await?;
        p.citas
            .registrar_evento(&mut cx, &self.evento(id, evento, actor))
            .await?;
        if nuevo == EstadoCita::NoAsistio {
            p.publicar(
                &mut cx,
                &InasistenciaMarcada {
                    cita: id,
                    cliente: cita.cliente,
                },
            )
            .await?;
        }
        let historial = p.citas.historial(&mut cx, id).await?;
        let actualizada = p.cita(&mut cx, id, false).await?;
        let dto = p.dto(&mut cx, actualizada, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(DetalleCitaDto { cita: dto, historial })
    }

    /// Confirmación manual, p. ej. por llamada (RN-31).
    pub(crate) async fn confirmar(&self, actor: &Actor, id: IdCita) -> Result<DetalleCitaDto, ErrorAgenda> {
        self.cambiar_estado(actor, id, EstadoCita::Confirmada, "confirmada")
            .await
    }

    /// «Iniciar»: pasa a «En curso» (RF-AGE-10).
    pub(crate) async fn iniciar(&self, actor: &Actor, id: IdCita) -> Result<DetalleCitaDto, ErrorAgenda> {
        self.cambiar_estado(actor, id, EstadoCita::EnCurso, "iniciada")
            .await
    }

    /// «No asistió»: suma una inasistencia al cliente (RF-AGE-08, evento `InasistenciaMarcada`).
    pub(crate) async fn inasistencia(
        &self,
        actor: &Actor,
        id: IdCita,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        self.cambiar_estado(actor, id, EstadoCita::NoAsistio, "no_asistio")
            .await
    }

    /// Citas pendientes que quedarían afectadas por un cambio (RF-BAR-13, RF-PBA-09).
    pub(crate) async fn conflictos(
        &self,
        actor: &Actor,
        barbero: IdBarbero,
        propuesta: Propuesta,
    ) -> Result<Vec<CitaDto>, ErrorAgenda> {
        administrador_o_propio(actor, barbero).map_err(|_| ErrorAgenda::Prohibido)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let filtro = FiltroCitas {
            desde: Some(p.reloj.ahora()),
            barbero: Some(barbero),
            estados: vec![EstadoCita::SinConfirmar, EstadoCita::Confirmada],
            ..Default::default()
        };
        let futuras = p.citas.listar(&mut cx, &filtro).await?;
        let afectadas: Vec<CitaGuardada> = futuras
            .into_iter()
            .filter(|c| {
                let (i, f) = (
                    c.inicio.with_timezone(&ZONA_HORARIA),
                    c.fin.with_timezone(&ZONA_HORARIA),
                );
                match &propuesta {
                    Propuesta::Ausencia { desde, hasta } => (*desde..=*hasta).contains(&i.date_naive()),
                    Propuesta::Bloqueo { inicio, fin } => c.inicio < *fin && c.fin > *inicio,
                    Propuesta::Horario(franjas) => {
                        let dia = u8::try_from(i.weekday().number_from_monday()).unwrap_or_default();
                        let (hi, hf): (NaiveTime, NaiveTime) = (i.time(), f.time());
                        !franjas
                            .iter()
                            .any(|fr| fr.dia_semana == dia && fr.inicio <= hi && hf <= fr.fin)
                    }
                }
            })
            .collect();
        let dtos = p.dtos(&mut cx, afectadas, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dtos)
    }

    /// Citas que ya terminaron sin venta ni inasistencia (RF-AGE-12).
    pub(crate) async fn pendientes_de_cierre(
        &self,
        actor: &Actor,
        fecha: Option<NaiveDate>,
    ) -> Result<Vec<CitaDto>, ErrorAgenda> {
        solo_administrador(actor)?;
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let filtro = FiltroCitas {
            desde: fecha.map(inicio_del_dia),
            hasta: Some(fecha.map_or(ahora, |f| (inicio_del_dia(f) + TimeDelta::days(1)).min(ahora))),
            barbero: None,
            estados: vec![
                EstadoCita::SinConfirmar,
                EstadoCita::Confirmada,
                EstadoCita::EnCurso,
            ],
        };
        let citas: Vec<CitaGuardada> = p
            .citas
            .listar(&mut cx, &filtro)
            .await?
            .into_iter()
            .filter(|c| c.fin <= ahora)
            .collect();
        let dtos = p.dtos(&mut cx, citas, true).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dtos)
    }

    pub(crate) async fn configuracion(&self, actor: &Actor) -> Result<ConfiguracionAgenda, ErrorAgenda> {
        solo_administrador(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let c = self.p.citas.configuracion(&mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(c)
    }

    /// Reglas de agendamiento (RF-CFG-02), auditadas.
    pub(crate) async fn guardar_configuracion(
        &self,
        actor: &Actor,
        nueva: ConfiguracionAgenda,
    ) -> Result<ConfiguracionAgenda, ErrorAgenda> {
        solo_administrador(actor)?;
        nueva.validar()?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes = p.citas.configuracion(&mut cx).await?;
        p.citas.guardar_configuracion(&mut cx, &nueva).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(
                    actor,
                    "agenda",
                    "cambiar_configuracion",
                    "configuracion_agenda",
                    1,
                )
                .antes(&antes)
                .despues(&nueva),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(nueva)
    }
}
