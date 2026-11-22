//! Reserva en línea (RF-RES-07 a 12, RF-SIS-02, RNF-INT-01).

use std::net::IpAddr;
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeDelta, Utc};
use clientes::api::DatosReservaEnLinea;
use contratos::agenda::CitaCreada;
use nucleo::identificadores::{IdBarbero, IdServicio};
use nucleo::{ErrorPersistencia, TipoActor};
use plataforma::seguridad::generar_token;

use super::concurrencia::con_reintentos;
use super::dto::CitaCreadaDto;
use super::errores::ErrorAgenda;
use super::puertos::{
    CitaGuardada, EventoCita, NuevaCita, Puertos, RESTRICCION_SOLAPE, alternativas, enlace_google_calendar,
};
use crate::dominio::cita::EstadoCita;
use crate::dominio::disponibilidad::Modo;

/// Vigencia del enlace de gestión después del fin de la cita.
pub(crate) const VIGENCIA_ENLACE_DESPUES: TimeDelta = TimeDelta::days(30);

#[derive(Debug, Clone)]
pub(crate) struct SolicitudReserva {
    pub(crate) comprobante: String,
    pub(crate) servicio_id: IdServicio,
    /// `None`: «Cualquier barbero».
    pub(crate) barbero_id: Option<IdBarbero>,
    pub(crate) inicio: DateTime<FixedOffset>,
    pub(crate) nombre: String,
    pub(crate) correo: Option<String>,
    pub(crate) fecha_nacimiento: Option<NaiveDate>,
    pub(crate) autoriza_tratamiento_datos: bool,
    pub(crate) politica_version_id: i64,
    pub(crate) ip: Option<IpAddr>,
    pub(crate) agente: Option<String>,
}

pub(crate) struct Reserva<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Reserva<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reserva")
    }
}

impl<C: Send + 'static> Reserva<C> {
    /// Crea la cita de forma atómica: consume el comprobante, revalida la hora, registra o
    /// actualiza al cliente con su autorización, guarda la cita, el enlace de gestión y la
    /// confirmación por WhatsApp. Si la hora se tomó entre medio, nada queda guardado (el
    /// comprobante sigue sirviendo) y el error trae horas alternativas.
    pub(crate) async fn crear(&self, s: SolicitudReserva) -> Result<CitaCreadaDto, ErrorAgenda> {
        con_reintentos(|| self.intentar_crear(s.clone())).await
    }

    async fn intentar_crear(&self, s: SolicitudReserva) -> Result<CitaCreadaDto, ErrorAgenda> {
        if !s.autoriza_tratamiento_datos {
            return Err(ErrorAgenda::campo(
                "autoriza_tratamiento_datos",
                "Acepta el tratamiento de tus datos para confirmar la reserva.",
            ));
        }
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let inicio = s.inicio.with_timezone(&Utc);
        let mut cx = p.transacciones.iniciar().await?;

        let politica_vigente = p
            .negocio
            .politicas_vigentes(&mut cx)
            .await?
            .into_iter()
            .find(|politica| politica.tipo == "tratamiento_datos");
        if politica_vigente
            .as_ref()
            .is_none_or(|politica| politica.id != s.politica_version_id)
        {
            return Err(ErrorAgenda::campo(
                "politica_version_id",
                "Acepta la versión vigente de la política de tratamiento de datos.",
            ));
        }
        let celular = p.comprobantes.consumir(&mut cx, &s.comprobante).await?;
        let servicio = p.servicio(&mut cx, s.servicio_id, Modo::Publico).await?;

        // RN-10: política de inasistencias.
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let conocido = p.clientes.por_celular(&mut cx, &celular).await?;
        let reincidente = conocido
            .as_ref()
            .is_some_and(|c| c.inasistencias >= i32::from(configuracion.umbral_inasistencias));
        let estado = match (reincidente, configuracion.politica_inasistencias.as_str()) {
            (true, "bloquear_reserva_en_linea") => return Err(ErrorAgenda::ReservaBloqueada),
            (true, "confirmar_por_llamada") => EstadoCita::SinConfirmar,
            _ => EstadoCita::Confirmada,
        };

        let (barbero, libres) = p
            .reservar_hora(&mut cx, &servicio, s.barbero_id, inicio, Modo::Publico, None)
            .await?;
        let cliente = p
            .clientes
            .registrar_en_linea(
                &mut cx,
                DatosReservaEnLinea {
                    celular,
                    nombre: s.nombre,
                    correo: s.correo,
                    fecha_nacimiento: s.fecha_nacimiento,
                    politica_version_id: s.politica_version_id,
                    ip: s.ip,
                    agente: s.agente,
                },
            )
            .await?;
        let fin = inicio + TimeDelta::minutes(i64::from(servicio.duracion_min.unwrap_or(30)));
        let nueva = NuevaCita {
            cliente,
            barbero,
            servicio: servicio.id,
            inicio,
            fin,
            precio: servicio.precio.valor(),
            estado,
            creada_por: None,
            asignacion_automatica: s.barbero_id.is_none(),
            nota: None,
            ahora,
        };
        let id = match p.citas.crear(&mut cx, &nueva).await {
            Ok(id) => id,
            // Otra persona tomó la hora entre la revalidación y el guardado (RNF-INT-01). Con
            // «Cualquier barbero» puede quedar otro libre: se reintenta con la disponibilidad nueva.
            Err(ErrorPersistencia::Conflicto { restriccion }) if restriccion == RESTRICCION_SOLAPE => {
                if s.barbero_id.is_none() {
                    return Err(ErrorAgenda::Persistencia(ErrorPersistencia::Concurrencia));
                }
                return Err(ErrorAgenda::ConflictoHorario {
                    alternativas: alternativas(&libres, inicio),
                });
            }
            Err(otro) => return Err(otro.into()),
        };
        let (token, huella) = generar_token().map_err(|e| ErrorAgenda::Interno(e.0))?;
        p.citas
            .guardar_enlace(&mut cx, id, &huella, fin + VIGENCIA_ENLACE_DESPUES, ahora)
            .await?;
        p.citas
            .registrar_evento(
                &mut cx,
                &EventoCita {
                    cita: id,
                    evento: "creada",
                    actor: TipoActor::Cliente,
                    usuario: None,
                    inicio_previo: None,
                    barbero_previo: None,
                    tardio: false,
                    detalle: None,
                    ahora,
                },
            )
            .await?;
        let cita = CitaGuardada {
            id,
            cliente,
            barbero,
            servicio: servicio.id,
            inicio,
            fin,
            precio: nueva.precio,
            estado,
            origen: "en_linea".into(),
            asignacion_automatica: nueva.asignacion_automatica,
            nota: None,
            cancelacion_tardia: None,
            motivo_cancelacion: None,
        };
        let enlace = p.enlace_gestion(&token);
        p.avisar(&mut cx, &cita, "confirmacion", Some(enlace.clone()))
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
        let negocio = p.negocio.negocio(&mut cx).await?;
        let dto = p.dto(&mut cx, cita, false).await?;
        p.transacciones.confirmar(cx).await?;
        let titulo = format!("{} · {}", dto.servicio.nombre, negocio.nombre_comercial);
        Ok(CitaCreadaDto {
            calendario_google: enlace_google_calendar(&titulo, inicio, fin, negocio.direccion.as_deref()),
            cita: dto,
            enlace_gestion: enlace,
        })
    }
}
