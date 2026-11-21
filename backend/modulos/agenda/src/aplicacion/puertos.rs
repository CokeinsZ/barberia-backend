//! Puertos del módulo (genéricos en el contexto `C`, ADR 0005) y utilidades que comparten los
//! casos de uso: carga de datos del motor, armado de las citas y avisos al cliente.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use barberos::api::{BarberoAgenda, EquipoAgenda};
use catalogo::api::{CatalogoServicios, ServicioResumen};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeDelta, Timelike, Utc};
use clientes::api::ClientesAgenda;
use negocio::api::{CalendarioNegocio, DatosPublicos, TipoDiaEspecialDto};
use notificaciones::api::{Avisos, BuzonSalida, ConsultaMensajes, PlantillaWhatsApp, WhatsAppSaliente};
use nucleo::eventos::{BusEventos, EventoDominio};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio, IdUsuario};
use nucleo::{Celular, Dinero, ErrorPersistencia, Reloj, TipoActor, Transacciones, ZONA_HORARIA};
use plataforma::seguridad::Huella;
use verificacion::api::Comprobantes;

use super::dto::{
    BarberoCitaDto, CitaDto, ClienteCitaDto, DetalleCitaDto, EventoCitaDto, MensajeCitaDto, ServicioCitaDto,
};
use super::errores::ErrorAgenda;
use crate::dominio::cita::EstadoCita;
use crate::dominio::configuracion::ConfiguracionAgenda;
use crate::dominio::disponibilidad::{self, Agendable, Consulta, Franja, Modo, Reglas, TipoDia};

pub(crate) const RESTRICCION_SOLAPE: &str = "cita_sin_solape";

/// Cita leída de la base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CitaGuardada {
    pub(crate) id: IdCita,
    pub(crate) cliente: IdCliente,
    pub(crate) barbero: IdBarbero,
    pub(crate) servicio: IdServicio,
    pub(crate) inicio: DateTime<Utc>,
    pub(crate) fin: DateTime<Utc>,
    pub(crate) precio: i64,
    pub(crate) estado: EstadoCita,
    pub(crate) origen: String,
    pub(crate) asignacion_automatica: bool,
    pub(crate) nota: Option<String>,
    pub(crate) cancelacion_tardia: Option<bool>,
    pub(crate) motivo_cancelacion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevaCita {
    pub(crate) cliente: IdCliente,
    pub(crate) barbero: IdBarbero,
    pub(crate) servicio: IdServicio,
    pub(crate) inicio: DateTime<Utc>,
    pub(crate) fin: DateTime<Utc>,
    pub(crate) precio: i64,
    pub(crate) estado: EstadoCita,
    /// `None`: la creó el cliente en línea.
    pub(crate) creada_por: Option<IdUsuario>,
    pub(crate) asignacion_automatica: bool,
    pub(crate) nota: Option<String>,
    pub(crate) ahora: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventoCita {
    pub(crate) cita: IdCita,
    /// Valor del enum `agenda.evento_cita`.
    pub(crate) evento: &'static str,
    pub(crate) actor: TipoActor,
    pub(crate) usuario: Option<IdUsuario>,
    pub(crate) inicio_previo: Option<DateTime<Utc>>,
    pub(crate) barbero_previo: Option<IdBarbero>,
    pub(crate) tardio: bool,
    pub(crate) detalle: Option<String>,
    pub(crate) ahora: DateTime<Utc>,
}

/// Cita candidata a recordatorio: desde cuándo tiene su hora actual y qué recordatorios se
/// enviaron ya para esa hora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CitaParaRecordar {
    pub(crate) cita: CitaGuardada,
    pub(crate) vigente_desde: DateTime<Utc>,
    pub(crate) recordatorios_enviados: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FiltroCitas {
    pub(crate) desde: Option<DateTime<Utc>>,
    pub(crate) hasta: Option<DateTime<Utc>>,
    pub(crate) barbero: Option<IdBarbero>,
    pub(crate) estados: Vec<EstadoCita>,
}

#[async_trait]
pub(crate) trait RepositorioAgenda<C: Send>: Send + Sync + fmt::Debug {
    async fn configuracion(&self, cx: &mut C) -> Result<ConfiguracionAgenda, ErrorPersistencia>;
    async fn guardar_configuracion(
        &self,
        cx: &mut C,
        configuracion: &ConfiguracionAgenda,
    ) -> Result<(), ErrorPersistencia>;
    /// Intervalos de las citas que ocupan horario (todas menos canceladas e inasistencias).
    async fn ocupacion(
        &self,
        cx: &mut C,
        barberos: &[IdBarbero],
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
        excepto: Option<IdCita>,
    ) -> Result<Vec<(IdBarbero, DateTime<Utc>, DateTime<Utc>)>, ErrorPersistencia>;
    /// `Conflicto { restriccion: "cita_sin_solape" }` si el barbero ya tiene una cita cruzada.
    async fn crear(&self, cx: &mut C, cita: &NuevaCita) -> Result<IdCita, ErrorPersistencia>;
    /// Con `bloquear`, la fila queda bloqueada hasta el fin de la transacción.
    async fn obtener(
        &self,
        cx: &mut C,
        id: IdCita,
        bloquear: bool,
    ) -> Result<Option<CitaGuardada>, ErrorPersistencia>;
    async fn listar(&self, cx: &mut C, filtro: &FiltroCitas) -> Result<Vec<CitaGuardada>, ErrorPersistencia>;
    async fn cambiar_estado(
        &self,
        cx: &mut C,
        id: IdCita,
        estado: EstadoCita,
    ) -> Result<(), ErrorPersistencia>;
    async fn cancelar(
        &self,
        cx: &mut C,
        id: IdCita,
        por: TipoActor,
        motivo: &str,
        tardia: bool,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
    /// `Conflicto { restriccion: "cita_sin_solape" }` si la nueva hora se cruza.
    async fn mover(
        &self,
        cx: &mut C,
        id: IdCita,
        barbero: IdBarbero,
        inicio: DateTime<Utc>,
        fin: DateTime<Utc>,
        asignacion_automatica: bool,
    ) -> Result<(), ErrorPersistencia>;
    async fn registrar_evento(&self, cx: &mut C, evento: &EventoCita) -> Result<(), ErrorPersistencia>;
    async fn historial(&self, cx: &mut C, id: IdCita) -> Result<Vec<EventoCitaDto>, ErrorPersistencia>;
    /// Crea o reemplaza el enlace de gestión (el anterior deja de servir).
    async fn guardar_enlace(
        &self,
        cx: &mut C,
        cita: IdCita,
        huella: &Huella,
        expira_en: DateTime<Utc>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
    async fn por_enlace(
        &self,
        cx: &mut C,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<IdCita>, ErrorPersistencia>;
    async fn vincular_mensaje(
        &self,
        cx: &mut C,
        cita: IdCita,
        proposito: &'static str,
        inicio: DateTime<Utc>,
        mensaje: i64,
    ) -> Result<(), ErrorPersistencia>;
    /// Citas pendientes que empiezan en `(desde, hasta]` (candidatas a recordatorio).
    async fn citas_para_recordar(
        &self,
        cx: &mut C,
        desde: DateTime<Utc>,
        hasta: DateTime<Utc>,
    ) -> Result<Vec<CitaParaRecordar>, ErrorPersistencia>;
    /// Mensajes relacionados con la cita: `(propósito, id del mensaje)`, del más antiguo al más nuevo.
    async fn mensajes_de_cita(
        &self,
        cx: &mut C,
        cita: IdCita,
    ) -> Result<Vec<(String, i64)>, ErrorPersistencia>;
    /// Citas «Sin confirmar», «Confirmada» o «En curso» del cliente.
    async fn pendientes_de_cliente(&self, cx: &mut C, cliente: IdCliente) -> Result<i64, ErrorPersistencia>;
    /// Cita y propósito del mensaje al que responde un cliente.
    async fn cita_por_mensaje(
        &self,
        cx: &mut C,
        mensaje: i64,
    ) -> Result<Option<(IdCita, String)>, ErrorPersistencia>;
}

/// Lo que comparten los casos de uso.
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) citas: Arc<dyn RepositorioAgenda<C>>,
    pub(crate) equipo: Arc<dyn EquipoAgenda<C>>,
    pub(crate) catalogo: Arc<dyn CatalogoServicios<C>>,
    pub(crate) calendario: Arc<dyn CalendarioNegocio<C>>,
    pub(crate) negocio: Arc<dyn DatosPublicos<C>>,
    pub(crate) clientes: Arc<dyn ClientesAgenda<C>>,
    pub(crate) comprobantes: Arc<dyn Comprobantes<C>>,
    pub(crate) buzon: Arc<dyn BuzonSalida<C>>,
    pub(crate) avisos: Arc<dyn Avisos<C>>,
    pub(crate) mensajes: Arc<dyn ConsultaMensajes<C>>,
    pub(crate) bus: Arc<BusEventos<C>>,
    pub(crate) auditor: Arc<dyn auditoria::api::Auditor<C>>,
    pub(crate) reloj: Arc<dyn Reloj>,
    /// URL pública del sitio, para los enlaces de gestión.
    pub(crate) url_publica: String,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PuertosAgenda")
    }
}

/// Qué se le pide al motor: servicio, barbero (o cualquiera), rango de fechas, quién consulta y
/// la cita que se está moviendo (su propio horario no cuenta como ocupado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PedidoMotor {
    pub(crate) servicio: IdServicio,
    pub(crate) barbero: Option<IdBarbero>,
    pub(crate) desde: NaiveDate,
    pub(crate) hasta: NaiveDate,
    pub(crate) modo: Modo,
    pub(crate) excepto: Option<IdCita>,
}

/// Datos que necesita el motor de disponibilidad para un rango de fechas.
#[derive(Debug, Clone)]
pub(crate) struct DatosMotor {
    /// Barberos que ofrecen el servicio (y, en el sitio público, reciben citas en línea), en el
    /// orden del equipo.
    pub(crate) barberos: Vec<(BarberoAgenda, Agendable)>,
    pub(crate) especiales: HashMap<NaiveDate, TipoDia>,
    pub(crate) reglas: Reglas,
}

impl DatosMotor {
    pub(crate) fn tipo_dia(&self, fecha: NaiveDate) -> TipoDia {
        self.especiales.get(&fecha).copied().unwrap_or(TipoDia::Normal)
    }

    pub(crate) fn horas(&self, fecha: NaiveDate, consulta: &Consulta) -> Vec<(DateTime<Utc>, IdBarbero)> {
        let agendables: Vec<Agendable> = self.barberos.iter().map(|(_, a)| a.clone()).collect();
        disponibilidad::horas(&agendables, fecha, self.tipo_dia(fecha), consulta)
    }

    pub(crate) fn nombre(&self, id: IdBarbero) -> String {
        self.barberos
            .iter()
            .find(|(b, _)| b.id == id)
            .map(|(b, _)| b.nombre.clone())
            .unwrap_or_default()
    }
}

pub(crate) fn reglas(c: &ConfiguracionAgenda) -> Reglas {
    Reglas {
        anticipacion: TimeDelta::minutes(i64::from(c.anticipacion_minima_min)),
        ventana_dias: i64::from(c.ventana_reserva_dias),
        intervalo: TimeDelta::minutes(i64::from(c.intervalo_min)),
    }
}

/// Instante en la zona de Bogotá, para las respuestas.
pub(crate) fn local(instante: DateTime<Utc>) -> DateTime<FixedOffset> {
    instante.with_timezone(&ZONA_HORARIA).fixed_offset()
}

/// Medianoche de una fecha de Bogotá, en UTC.
pub(crate) fn inicio_del_dia(fecha: NaiveDate) -> DateTime<Utc> {
    fecha
        .and_hms_opt(0, 0, 0)
        .and_then(|d| d.and_local_timezone(ZONA_HORARIA).earliest())
        .map_or_else(|| DateTime::<Utc>::MIN_UTC, |d| d.with_timezone(&Utc))
}

const DIAS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];
const MESES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

/// «jueves 19 de noviembre» y «3:30 p. m.», para los mensajes.
pub(crate) fn fecha_y_hora(instante: DateTime<Utc>) -> (String, String) {
    let l = instante.with_timezone(&ZONA_HORARIA);
    let fecha = format!(
        "{} {} de {}",
        DIAS[l.weekday().num_days_from_monday() as usize],
        l.day(),
        MESES[l.month0() as usize]
    );
    let (pm, hora12) = l.hour12();
    let hora = format!(
        "{}:{:02} {}",
        hora12,
        l.minute(),
        if pm { "p. m." } else { "a. m." }
    );
    (fecha, hora)
}

/// Codificación de URL (RFC 3986, caracteres no reservados sin codificar).
pub(crate) fn codificar_url(texto: &str) -> String {
    texto
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Enlace de Google Calendar con el servicio, el inicio, el fin y la zona (RF-RES-13).
pub(crate) fn enlace_google_calendar(
    titulo: &str,
    inicio: DateTime<Utc>,
    fin: DateTime<Utc>,
    lugar: Option<&str>,
) -> String {
    let formato = "%Y%m%dT%H%M%SZ";
    format!(
        "https://calendar.google.com/calendar/render?action=TEMPLATE&text={}&dates={}/{}&ctz=America%2FBogota&location={}",
        codificar_url(titulo),
        inicio.format(formato),
        fin.format(formato),
        codificar_url(lugar.unwrap_or_default()),
    )
}

impl<C: Send> Puertos<C> {
    pub(crate) fn enlace_gestion(&self, token: &str) -> String {
        format!("{}/cita/{token}", self.url_publica.trim_end_matches('/'))
    }

    /// El servicio que se quiere agendar: activo, con duración y, en línea, agendable.
    pub(crate) async fn servicio(
        &self,
        cx: &mut C,
        id: IdServicio,
        modo: Modo,
    ) -> Result<ServicioResumen, ErrorAgenda> {
        let servicio = self.catalogo.resumenes(cx, &[id]).await?.into_iter().next();
        match servicio {
            Some(s)
                if s.activo
                    && s.duracion_min.is_some()
                    && (modo == Modo::Personal || s.agendable_en_linea) =>
            {
                Ok(s)
            }
            _ => Err(ErrorAgenda::campo(
                "servicio_id",
                "Elige un servicio que se pueda agendar.",
            )),
        }
    }

    /// Equipo, días especiales y ocupación para el rango, listos para el motor.
    pub(crate) async fn datos_motor(
        &self,
        cx: &mut C,
        pedido: PedidoMotor,
    ) -> Result<DatosMotor, ErrorAgenda> {
        let PedidoMotor {
            servicio,
            barbero,
            desde,
            hasta,
            modo,
            excepto,
        } = pedido;
        let configuracion = self.citas.configuracion(cx).await?;
        let equipo: Vec<BarberoAgenda> = self
            .equipo
            .equipo(cx, desde, hasta)
            .await?
            .into_iter()
            .filter(|b| b.servicios.contains(&servicio))
            .filter(|b| modo == Modo::Personal || b.recibe_citas_en_linea)
            .filter(|b| barbero.is_none_or(|id| id == b.id))
            .collect();
        let ids: Vec<IdBarbero> = equipo.iter().map(|b| b.id).collect();
        let desde_utc = inicio_del_dia(desde) - TimeDelta::days(1);
        let hasta_utc = inicio_del_dia(hasta) + TimeDelta::days(2);
        let ocupacion = self
            .citas
            .ocupacion(cx, &ids, desde_utc, hasta_utc, excepto)
            .await?;
        let especiales = self
            .calendario
            .dias_especiales(cx, desde, hasta)
            .await?
            .into_iter()
            .map(|d| {
                let tipo = match (d.tipo, d.hora_cierre) {
                    (TipoDiaEspecialDto::Festivo, _) => TipoDia::Festivo,
                    (TipoDiaEspecialDto::Cerrado, _) => TipoDia::Cerrado,
                    (TipoDiaEspecialDto::CierreTemprano, Some(hora)) => TipoDia::CierreTemprano(hora),
                    (TipoDiaEspecialDto::CierreTemprano, None) => TipoDia::Normal,
                };
                (d.fecha, tipo)
            })
            .collect();
        let barberos = equipo
            .into_iter()
            .map(|b| {
                let citas = ocupacion
                    .iter()
                    .filter(|(id, _, _)| *id == b.id)
                    .map(|(_, i, f)| (*i, *f));
                let agendable = a_agendable(&b, citas);
                (b, agendable)
            })
            .collect();
        Ok(DatosMotor {
            barberos,
            especiales,
            reglas: reglas(&configuracion),
        })
    }

    pub(crate) fn consulta(&self, servicio: &ServicioResumen, reglas: Reglas, modo: Modo) -> Consulta {
        Consulta {
            ahora: self.reloj.ahora(),
            duracion: TimeDelta::minutes(i64::from(servicio.duracion_min.unwrap_or(30))),
            reglas,
            modo,
        }
    }

    /// Comprueba que la hora pedida esté libre (con el barbero pedido o, si no, con el primero
    /// libre según el orden del equipo). Si no lo está, el error trae hasta 3 alternativas
    /// cercanas del mismo día.
    pub(crate) async fn reservar_hora(
        &self,
        cx: &mut C,
        servicio: &ServicioResumen,
        barbero: Option<IdBarbero>,
        inicio: DateTime<Utc>,
        modo: Modo,
        excepto: Option<IdCita>,
    ) -> Result<(IdBarbero, Vec<DateTime<Utc>>), ErrorAgenda> {
        let fecha = inicio.with_timezone(&ZONA_HORARIA).date_naive();
        let motor = self
            .datos_motor(
                cx,
                PedidoMotor {
                    servicio: servicio.id,
                    barbero,
                    desde: fecha,
                    hasta: fecha,
                    modo,
                    excepto,
                },
            )
            .await?;
        let consulta = self.consulta(servicio, motor.reglas, modo);
        let horas = motor.horas(fecha, &consulta);
        let todas: Vec<DateTime<Utc>> = horas.iter().map(|(h, _)| *h).collect();
        match horas.iter().find(|(h, _)| *h == inicio) {
            Some((_, elegido)) => Ok((*elegido, todas)),
            None => Err(ErrorAgenda::ConflictoHorario {
                alternativas: alternativas(&todas, inicio),
            }),
        }
    }

    /// Citas con nombres de servicio, barbero y (para el personal) cliente.
    pub(crate) async fn dtos(
        &self,
        cx: &mut C,
        citas: Vec<CitaGuardada>,
        con_cliente: bool,
    ) -> Result<Vec<CitaDto>, ErrorAgenda> {
        let mut servicios: Vec<IdServicio> = citas.iter().map(|c| c.servicio).collect();
        servicios.sort();
        servicios.dedup();
        let mut barberos: Vec<IdBarbero> = citas.iter().map(|c| c.barbero).collect();
        barberos.sort();
        barberos.dedup();
        let servicios: HashMap<IdServicio, ServicioResumen> = self
            .catalogo
            .resumenes(cx, &servicios)
            .await?
            .into_iter()
            .map(|s| (s.id, s))
            .collect();
        let nombres = self.equipo.nombres(cx, &barberos).await?;
        let clientes: HashMap<IdCliente, ClienteCitaDto> = if con_cliente {
            let mut ids: Vec<IdCliente> = citas.iter().map(|c| c.cliente).collect();
            ids.sort();
            ids.dedup();
            self.clientes
                .por_ids(cx, &ids)
                .await?
                .into_iter()
                .map(|c| {
                    (
                        c.id,
                        ClienteCitaDto {
                            id: c.id,
                            nombre: c.nombre,
                            celular: c.celular,
                            visitas: c.visitas,
                            inasistencias: c.inasistencias,
                        },
                    )
                })
                .collect()
        } else {
            HashMap::new()
        };
        Ok(citas
            .into_iter()
            .map(|c| CitaDto {
                id: c.id,
                estado: c.estado.codigo(),
                origen: c.origen,
                inicio: local(c.inicio),
                fin: local(c.fin),
                precio: Dinero::pesos(c.precio).unwrap_or_default(),
                servicio: ServicioCitaDto {
                    id: c.servicio,
                    nombre: servicios
                        .get(&c.servicio)
                        .map(|s| s.nombre.clone())
                        .unwrap_or_default(),
                    duracion_min: (c.fin - c.inicio).num_minutes(),
                },
                barbero: BarberoCitaDto {
                    id: c.barbero,
                    nombre: nombres.get(&c.barbero).cloned().unwrap_or_default(),
                    asignacion_automatica: c.asignacion_automatica,
                },
                cliente: clientes.get(&c.cliente).cloned(),
                nota: if con_cliente { c.nota } else { None },
                cancelacion_tardia: c.cancelacion_tardia,
                motivo_cancelacion: c.motivo_cancelacion,
            })
            .collect())
    }

    pub(crate) async fn dto(
        &self,
        cx: &mut C,
        cita: CitaGuardada,
        con_cliente: bool,
    ) -> Result<CitaDto, ErrorAgenda> {
        self.dtos(cx, vec![cita], con_cliente)
            .await?
            .pop()
            .ok_or(ErrorAgenda::CitaNoEncontrada)
    }

    /// Detalle para el personal: la cita con su cliente, su historial y el estado de sus
    /// mensajes de WhatsApp.
    pub(crate) async fn detalle(
        &self,
        cx: &mut C,
        cita: CitaGuardada,
    ) -> Result<DetalleCitaDto, ErrorAgenda> {
        let historial = self.citas.historial(cx, cita.id).await?;
        let vinculos = self.citas.mensajes_de_cita(cx, cita.id).await?;
        let ids: Vec<i64> = vinculos.iter().map(|(_, id)| *id).collect();
        let estados = self.mensajes.estados(cx, &ids).await?;
        let mensajes = vinculos
            .into_iter()
            .filter_map(|(proposito, id)| {
                estados.iter().find(|e| e.id == id).map(|e| MensajeCitaDto {
                    proposito,
                    plantilla: e.plantilla.clone(),
                    estado: e.estado.clone(),
                    intentos: e.intentos,
                    ultimo_error: e.ultimo_error.clone(),
                    creado_en: local(e.creado_en),
                    enviado_en: e.enviado_en.map(local),
                })
            })
            .collect();
        let dto = self.dto(cx, cita, true).await?;
        Ok(DetalleCitaDto {
            cita: dto,
            historial,
            mensajes,
        })
    }

    /// Encola el WhatsApp para el cliente y lo relaciona con la cita.
    pub(crate) async fn avisar(
        &self,
        cx: &mut C,
        cita: &CitaGuardada,
        proposito: &'static str,
        enlace: Option<String>,
    ) -> Result<(), ErrorAgenda> {
        let cliente = self
            .clientes
            .por_ids(cx, &[cita.cliente])
            .await?
            .into_iter()
            .next();
        let Some(cliente) = cliente else { return Ok(()) };
        let Ok(celular) = Celular::nuevo(&cliente.celular) else {
            return Ok(());
        };
        let dto = self.dto(cx, cita.clone(), false).await?;
        let (fecha, hora) = fecha_y_hora(cita.inicio);
        let nombre = cliente
            .nombre
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let plantilla = match proposito {
            "cancelacion" => PlantillaWhatsApp::CitaCancelada {
                nombre,
                servicio: dto.servicio.nombre,
                fecha,
                hora,
            },
            "recordatorio_24h" | "recordatorio_2h" | "recordatorio_manual" => {
                PlantillaWhatsApp::RecordatorioCita {
                    nombre,
                    servicio: dto.servicio.nombre,
                    barbero: dto.barbero.nombre,
                    fecha,
                    hora,
                }
            }
            "aviso_reprogramacion" => PlantillaWhatsApp::AvisoReprogramacion {
                nombre,
                servicio: dto.servicio.nombre,
                fecha,
                hora,
                enlace: enlace.unwrap_or_default(),
            },
            "reprogramacion" => PlantillaWhatsApp::CitaReprogramada {
                nombre,
                servicio: dto.servicio.nombre,
                barbero: dto.barbero.nombre,
                fecha,
                hora,
                enlace: enlace.unwrap_or_default(),
            },
            _ => PlantillaWhatsApp::CitaConfirmada {
                nombre,
                servicio: dto.servicio.nombre,
                barbero: dto.barbero.nombre,
                fecha,
                hora,
                enlace: enlace.unwrap_or_default(),
            },
        };
        let mensaje = self
            .buzon
            .encolar_whatsapp(
                cx,
                WhatsAppSaliente {
                    destinatario: celular,
                    plantilla,
                },
            )
            .await?;
        self.citas
            .vincular_mensaje(cx, cita.id, proposito, cita.inicio, mensaje)
            .await?;
        Ok(())
    }

    pub(crate) async fn cita(
        &self,
        cx: &mut C,
        id: IdCita,
        bloquear: bool,
    ) -> Result<CitaGuardada, ErrorAgenda> {
        self.citas
            .obtener(cx, id, bloquear)
            .await?
            .ok_or(ErrorAgenda::CitaNoEncontrada)
    }
}

impl<C: Send + 'static> Puertos<C> {
    pub(crate) async fn publicar<E: EventoDominio>(&self, cx: &mut C, evento: &E) -> Result<(), ErrorAgenda> {
        self.bus
            .publicar(cx, evento)
            .await
            .map_err(|e| ErrorAgenda::Interno(e.to_string()))
    }
}

/// El barbero como lo ve el motor: franjas, ausencias, retiro y lo ocupado (bloqueos + citas).
pub(crate) fn a_agendable(
    b: &BarberoAgenda,
    citas: impl Iterator<Item = (DateTime<Utc>, DateTime<Utc>)>,
) -> Agendable {
    let mut ocupado = b.bloqueos.clone();
    ocupado.extend(citas);
    Agendable {
        id: b.id,
        franjas: b
            .franjas
            .iter()
            .map(|f| Franja {
                dia_semana: f.dia_semana,
                inicio: f.inicio,
                fin: f.fin,
            })
            .collect(),
        ausencias: b.ausencias.iter().map(|a| (a.desde, a.hasta)).collect(),
        fecha_retiro: b.fecha_retiro,
        ocupado,
    }
}

/// Hasta 3 horas libres, las más cercanas a la pedida, en orden.
pub(crate) fn alternativas(libres: &[DateTime<Utc>], pedida: DateTime<Utc>) -> Vec<DateTime<FixedOffset>> {
    let mut cercanas: Vec<DateTime<Utc>> = libres.iter().copied().filter(|h| *h != pedida).collect();
    cercanas.sort_by_key(|h| (*h - pedida).num_seconds().abs());
    cercanas.truncate(3);
    cercanas.sort();
    cercanas.into_iter().map(local).collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn redacta_fechas_y_horas_para_los_mensajes() {
        let instante: DateTime<Utc> = "2026-11-19T20:30:00Z".parse().unwrap();
        assert_eq!(
            fecha_y_hora(instante),
            ("jueves 19 de noviembre".to_owned(), "3:30 p. m.".to_owned())
        );
        let manana: DateTime<Utc> = "2026-11-20T14:05:00Z".parse().unwrap();
        assert_eq!(fecha_y_hora(manana).1, "9:05 a. m.");
    }

    #[test]
    fn propone_las_alternativas_mas_cercanas() {
        let h = |t: &str| t.parse::<DateTime<Utc>>().unwrap();
        let libres = [
            h("2026-11-19T14:00:00Z"),
            h("2026-11-19T19:00:00Z"),
            h("2026-11-19T20:00:00Z"),
            h("2026-11-19T21:00:00Z"),
            h("2026-11-19T23:00:00Z"),
        ];
        let propuestas = alternativas(&libres, h("2026-11-19T20:30:00Z"));
        let horas: Vec<String> = propuestas.iter().map(|d| d.format("%H:%M").to_string()).collect();
        assert_eq!(horas, ["14:00", "15:00", "16:00"]);
    }

    #[test]
    fn arma_el_enlace_de_google_calendar() {
        let inicio: DateTime<Utc> = "2026-11-19T20:30:00Z".parse().unwrap();
        let enlace = enlace_google_calendar(
            "Corte · Diamante",
            inicio,
            inicio + TimeDelta::minutes(45),
            Some("Cra 15 # 93"),
        );
        assert!(enlace.contains("dates=20261119T203000Z/20261119T211500Z"));
        assert!(enlace.contains("ctz=America%2FBogota"));
        assert!(enlace.contains("text=Corte%20%C2%B7%20Diamante"));
    }
}
