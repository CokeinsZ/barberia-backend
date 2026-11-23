//! «Mis citas» del barbero (RF-PBA-02): citas por estado, tiempo libre y días con la agenda
//! llena de hoy, mañana y los próximos 7 días. El tiempo y las horas libres salen de las mismas
//! reglas que el agendamiento (franjas, festivos, cierres, ausencias, bloqueos y citas).

use std::sync::Arc;

use chrono::{Days, NaiveDate, TimeDelta};
use nucleo::identificadores::IdBarbero;
use nucleo::{Actor, ZONA_HORARIA};

use super::dto::{ConteoCitasDto, DiaBarberoDto, ProximosDiasDto, ResumenAgendaBarberoDto};
use super::errores::ErrorAgenda;
use super::puertos::{CitaGuardada, FiltroCitas, Puertos, a_agendable, inicio_del_dia, reglas, tipo_de_dia};
use crate::dominio::cita::EstadoCita;
use crate::dominio::disponibilidad::{Consulta, Modo, franjas_del_dia, horas_del_barbero};
use crate::dominio::resumen::{en_el_dia, minutos, minutos_del_dia};

/// Días que cubre «Próximos 7 días», sin contar hoy.
const PROXIMOS_DIAS: u64 = 7;

#[derive(Debug)]
pub(crate) struct ResumenBarbero<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

/// Citas que ocupan tiempo: las vigentes y las ya atendidas.
fn ocupa(estado: EstadoCita) -> bool {
    matches!(
        estado,
        EstadoCita::SinConfirmar | EstadoCita::Confirmada | EstadoCita::EnCurso | EstadoCita::Atendida
    )
}

fn contar<'a>(citas: impl Iterator<Item = &'a CitaGuardada>) -> ConteoCitasDto {
    let mut c = ConteoCitasDto::default();
    for cita in citas {
        match cita.estado {
            EstadoCita::SinConfirmar => c.sin_confirmar += 1,
            EstadoCita::Confirmada => c.confirmadas += 1,
            EstadoCita::EnCurso => c.en_curso += 1,
            EstadoCita::Atendida => c.atendidas += 1,
            EstadoCita::NoAsistio => c.no_asistio += 1,
            EstadoCita::Cancelada => c.canceladas += 1,
        }
    }
    c.total = c.sin_confirmar + c.confirmadas + c.en_curso + c.atendidas + c.no_asistio;
    c.por_atender = c.sin_confirmar + c.confirmadas + c.en_curso;
    c
}

impl<C: Send> ResumenBarbero<C> {
    /// El barbero ve el suyo; la administradora, el del barbero que indique.
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        barbero: Option<IdBarbero>,
    ) -> Result<ResumenAgendaBarberoDto, ErrorAgenda> {
        let barbero = match actor {
            Actor::Barbero { barbero, .. } => *barbero,
            Actor::Administrador { .. } => {
                barbero.ok_or_else(|| ErrorAgenda::campo("barbero_id", "Indica el barbero."))?
            }
            Actor::Cliente { .. } | Actor::Sistema => return Err(ErrorAgenda::Prohibido),
        };
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let hoy = ahora.with_timezone(&ZONA_HORARIA).date_naive();
        let hasta = hoy
            .checked_add_days(Days::new(PROXIMOS_DIAS))
            .ok_or_else(|| ErrorAgenda::Interno("fecha fuera de rango".into()))?;

        let mut cx = p.transacciones.iniciar().await?;
        let Some(b) = p
            .equipo
            .equipo(&mut cx, hoy, hasta)
            .await?
            .into_iter()
            .find(|b| b.id == barbero)
        else {
            return Err(ErrorAgenda::campo(
                "barbero_id",
                "El barbero no existe o ya no está en el equipo.",
            ));
        };
        let citas = p
            .citas
            .listar(
                &mut cx,
                &FiltroCitas {
                    desde: Some(inicio_del_dia(hoy)),
                    hasta: Some(inicio_del_dia(hasta) + TimeDelta::days(1)),
                    barbero: Some(barbero),
                    estados: Vec::new(),
                },
            )
            .await?;
        let especiales = p.calendario.dias_especiales(&mut cx, hoy, hasta).await?;
        let configuracion = p.citas.configuracion(&mut cx).await?;
        // Su servicio más corto (activo y con duración): con él se decide si aún cabe algo.
        let duracion_minima = p
            .catalogo
            .resumenes(&mut cx, &b.servicios)
            .await?
            .iter()
            .filter(|s| s.activo)
            .filter_map(|s| s.duracion_min)
            .min();
        p.transacciones.confirmar(cx).await?;

        let ocupadas = citas
            .iter()
            .filter(|c| ocupa(c.estado))
            .map(|c| (c.inicio, c.fin));
        let agendable = a_agendable(&b, ocupadas);
        let consulta = duracion_minima.map(|minutos| Consulta {
            ahora,
            duracion: TimeDelta::minutes(i64::from(minutos)),
            reglas: reglas(&configuracion),
            modo: Modo::Personal,
        });
        let dia = |fecha: NaiveDate| -> DiaBarberoDto {
            let (tipo, tipo_dia) = tipo_de_dia(especiales.iter().find(|d| d.fecha == fecha));
            let medianoche = inicio_del_dia(fecha);
            let del_dia: Vec<&CitaGuardada> = citas
                .iter()
                .filter(|c| c.inicio.with_timezone(&ZONA_HORARIA).date_naive() == fecha)
                .collect();
            let franjas: Vec<(u32, u32)> = franjas_del_dia(&agendable, fecha, tipo)
                .into_iter()
                .map(|(inicio, fin)| (minutos(inicio), minutos(fin)))
                .collect();
            let bloqueos: Vec<(u32, u32)> = b
                .bloqueos
                .iter()
                .filter_map(|(inicio, fin)| en_el_dia(medianoche, *inicio, *fin))
                .collect();
            let ocupadas: Vec<(u32, u32)> = del_dia
                .iter()
                .filter(|c| ocupa(c.estado))
                .filter_map(|c| en_el_dia(medianoche, c.inicio, c.fin))
                .collect();
            let tiempo = minutos_del_dia(&franjas, &bloqueos, &ocupadas);
            let trabaja = !franjas.is_empty();
            let estado = if b.fecha_retiro.is_some_and(|retiro| fecha >= retiro) {
                "retirado".to_owned()
            } else if let Some(ausencia) = b.ausencias.iter().find(|a| a.desde <= fecha && fecha <= a.hasta) {
                ausencia.tipo.clone()
            } else if trabaja {
                "trabaja".to_owned()
            } else {
                "no_trabaja".to_owned()
            };
            let agenda_llena = trabaja
                && match &consulta {
                    Some(c) => horas_del_barbero(&agendable, fecha, tipo, c).is_empty(),
                    None => tiempo.libres() == 0,
                };
            DiaBarberoDto {
                fecha,
                tipo_dia,
                estado,
                citas: contar(del_dia.into_iter()),
                minutos_disponibles: tiempo.disponibles,
                minutos_ocupados: tiempo.ocupados,
                minutos_libres: trabaja.then(|| tiempo.libres()),
                agenda_llena,
            }
        };
        let dias: Vec<DiaBarberoDto> = hoy.iter_days().take_while(|f| *f <= hasta).map(dia).collect();
        let (hoy_dto, siguientes) = dias
            .split_first()
            .ok_or_else(|| ErrorAgenda::Interno("sin días".into()))?;
        let mut total = ConteoCitasDto::default();
        for d in siguientes {
            total.sumar(&d.citas);
        }
        Ok(ResumenAgendaBarberoDto {
            barbero_id: b.id,
            nombre: b.nombre.clone(),
            hoy: hoy_dto.clone(),
            manana: siguientes[0].clone(),
            proximos_7_dias: ProximosDiasDto {
                desde: siguientes[0].fecha,
                hasta,
                citas: total,
                dias_con_agenda_llena: siguientes.iter().filter(|d| d.agenda_llena).count(),
                dias: siguientes.to_vec(),
            },
        })
    }
}
