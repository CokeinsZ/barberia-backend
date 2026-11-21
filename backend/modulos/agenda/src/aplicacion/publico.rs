//! Consultas del sitio público: landing y pasos 1 y 2 del agendamiento.

use std::sync::Arc;

use chrono::{Datelike, Days, NaiveDate, Timelike};
use nucleo::ZONA_HORARIA;
use nucleo::identificadores::{IdBarbero, IdServicio};

use super::dto::{
    BarberoPublicoDto, CalendarioDto, DiaCalendarioDto, HoraLibreDto, HorasDelDiaDto, LandingDto,
    OpcionBarberoDto,
};
use super::errores::ErrorAgenda;
use super::puertos::{PedidoMotor, Puertos, local};
use crate::dominio::disponibilidad::{EstadoDia, Modo};

/// Estado de hoy según las ausencias: `None` si trabaja.
fn ausencia_hoy(b: &barberos::api::BarberoAgenda, hoy: NaiveDate) -> Option<&barberos::api::AusenciaAgenda> {
    b.ausencias.iter().find(|a| (a.desde..=a.hasta).contains(&hoy))
}

pub(crate) struct Publico<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Publico<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Publico")
    }
}

impl<C: Send> Publico<C> {
    /// Landing en una sola llamada (RF-PUB-01 a 04). Barberos: visibles y vigentes; los de
    /// vacaciones con su fecha de regreso y sin agendar; incapacidad, licencia y suspensión
    /// no se muestran (RN-27).
    pub(crate) async fn landing(&self) -> Result<LandingDto, ErrorAgenda> {
        let p = &self.p;
        let hoy = p.reloj.hoy();
        let mut cx = p.transacciones.iniciar().await?;
        let negocio = p.negocio.negocio(&mut cx).await?;
        let horario_atencion = p.negocio.horario_atencion(&mut cx).await?;
        let politicas = p.negocio.politicas_vigentes(&mut cx).await?;
        let servicios = p
            .catalogo
            .listado(&mut cx, false)
            .await?
            .into_iter()
            .filter(|s| s.visible_en_landing)
            .collect();
        let equipo = p.equipo.equipo(&mut cx, hoy, hoy).await?;
        p.transacciones.confirmar(cx).await?;
        let barberos = equipo
            .iter()
            .filter(|b| b.mostrar_en_landing)
            .filter_map(|b| {
                let ausencia = ausencia_hoy(b, hoy);
                if ausencia.is_some_and(|a| a.tipo != "vacaciones") {
                    return None;
                }
                Some(BarberoPublicoDto {
                    id: b.id,
                    nombre: b.nombre.clone(),
                    nombre_corto: b.nombres.split_whitespace().next().unwrap_or_default().to_owned(),
                    especialidad: b.especialidad.clone(),
                    anios_experiencia: b.anios_experiencia,
                    instagram: b.instagram.clone(),
                    biografia: b.biografia.clone(),
                    foto_url: b.foto_url.clone(),
                    de_vacaciones_hasta: ausencia.map(|a| a.hasta),
                    agendable: ausencia.is_none() && b.recibe_citas_en_linea,
                })
            })
            .collect();
        Ok(LandingDto {
            negocio,
            horario_atencion,
            servicios,
            barberos,
            politicas,
        })
    }

    /// Servicios y combos que se pueden agendar en línea y que al menos un barbero que recibe
    /// citas en línea ofrece (RF-RES-02).
    pub(crate) async fn servicios(&self) -> Result<Vec<catalogo::api::ServicioDto>, ErrorAgenda> {
        let p = &self.p;
        let hoy = p.reloj.hoy();
        let mut cx = p.transacciones.iniciar().await?;
        let servicios = p.catalogo.listado(&mut cx, false).await?;
        let equipo = p.equipo.equipo(&mut cx, hoy, hoy).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(servicios
            .into_iter()
            .filter(|s| s.agendable_en_linea && s.duracion_min.is_some())
            .filter(|s| {
                equipo
                    .iter()
                    .any(|b| b.recibe_citas_en_linea && b.servicios.contains(&s.id))
            })
            .collect())
    }

    /// Opciones de barbero para un servicio, con el motivo de las deshabilitadas y su próxima
    /// hora libre (RF-RES-03, RF-RES-04).
    pub(crate) async fn barberos(
        &self,
        servicio_id: IdServicio,
    ) -> Result<Vec<OpcionBarberoDto>, ErrorAgenda> {
        let p = &self.p;
        let hoy = p.reloj.hoy();
        let mut cx = p.transacciones.iniciar().await?;
        let servicio = p.servicio(&mut cx, servicio_id, Modo::Publico).await?;
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let hasta = hoy
            .checked_add_days(Days::new(
                u64::try_from(configuracion.ventana_reserva_dias).unwrap_or(30),
            ))
            .unwrap_or(hoy);
        let equipo = p.equipo.equipo(&mut cx, hoy, hoy).await?;
        let motor = p
            .datos_motor(
                &mut cx,
                PedidoMotor {
                    servicio: servicio_id,
                    barbero: None,
                    desde: hoy,
                    hasta,
                    modo: Modo::Publico,
                    excepto: None,
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        let consulta = p.consulta(&servicio, motor.reglas, Modo::Publico);
        let mut opciones = Vec::new();
        for b in equipo.iter().filter(|b| b.mostrar_en_landing) {
            let ausencia = ausencia_hoy(b, hoy);
            if ausencia.is_some_and(|a| a.tipo != "vacaciones") {
                continue;
            }
            let motivo = if !b.servicios.contains(&servicio_id) {
                Some("No realiza este servicio".to_owned())
            } else if let Some(a) = ausencia {
                Some(format!("De vacaciones hasta el {}", a.hasta.format("%d/%m/%Y")))
            } else if !b.recibe_citas_en_linea {
                Some("No recibe citas en línea".to_owned())
            } else {
                None
            };
            let proxima_hora = if motivo.is_none() {
                motor
                    .barberos
                    .iter()
                    .find(|(x, _)| x.id == b.id)
                    .and_then(|(_, agendable)| {
                        hoy.iter_days().take_while(|f| *f <= hasta).find_map(|fecha| {
                            crate::dominio::disponibilidad::horas_del_barbero(
                                agendable,
                                fecha,
                                motor.tipo_dia(fecha),
                                &consulta,
                            )
                            .first()
                            .copied()
                        })
                    })
            } else {
                None
            };
            opciones.push(OpcionBarberoDto {
                id: b.id,
                nombre: b.nombre.clone(),
                especialidad: b.especialidad.clone(),
                foto_url: b.foto_url.clone(),
                disponible: motivo.is_none() && proxima_hora.is_some(),
                motivo: motivo.or_else(|| {
                    proxima_hora
                        .is_none()
                        .then(|| "Sin horas libres en los próximos días".to_owned())
                }),
                proxima_hora: proxima_hora.map(local),
            });
        }
        Ok(opciones)
    }

    /// Calendario del mes (`2026-11`): estado de cada día (RF-RES-05). Sin barbero, «Cualquier
    /// barbero».
    pub(crate) async fn calendario(
        &self,
        servicio_id: IdServicio,
        barbero: Option<IdBarbero>,
        mes: &str,
    ) -> Result<CalendarioDto, ErrorAgenda> {
        let primero = NaiveDate::parse_from_str(&format!("{mes}-01"), "%Y-%m-%d")
            .map_err(|_| ErrorAgenda::campo("mes", "Escribe el mes como AAAA-MM."))?;
        let ultimo = primero
            .checked_add_months(chrono::Months::new(1))
            .and_then(|d| d.pred_opt())
            .unwrap_or(primero);
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let servicio = p.servicio(&mut cx, servicio_id, Modo::Publico).await?;
        let motor = p
            .datos_motor(
                &mut cx,
                PedidoMotor {
                    servicio: servicio_id,
                    barbero,
                    desde: primero,
                    hasta: ultimo,
                    modo: Modo::Publico,
                    excepto: None,
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        let consulta = p.consulta(&servicio, motor.reglas, Modo::Publico);
        let dias = primero
            .iter_days()
            .take_while(|f| *f <= ultimo)
            .map(|fecha| {
                if !consulta.fecha_permitida(fecha) {
                    return DiaCalendarioDto {
                        fecha,
                        estado: EstadoDia::FueraDeVentana.codigo(),
                        horas_libres: 0,
                    };
                }
                let n = motor.horas(fecha, &consulta).len();
                DiaCalendarioDto {
                    fecha,
                    estado: EstadoDia::por_cantidad(n).codigo(),
                    horas_libres: n,
                }
            })
            .collect();
        Ok(CalendarioDto {
            mes: format!("{:04}-{:02}", primero.year(), primero.month()),
            dias,
        })
    }

    /// Horas libres de un día, agrupadas en mañana (antes de las 12), tarde (12 a 18) y noche.
    pub(crate) async fn horas(
        &self,
        servicio_id: IdServicio,
        barbero: Option<IdBarbero>,
        fecha: NaiveDate,
        modo: Modo,
    ) -> Result<HorasDelDiaDto, ErrorAgenda> {
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let servicio = p.servicio(&mut cx, servicio_id, modo).await?;
        let motor = p
            .datos_motor(
                &mut cx,
                PedidoMotor {
                    servicio: servicio_id,
                    barbero,
                    desde: fecha,
                    hasta: fecha,
                    modo,
                    excepto: None,
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        let consulta = p.consulta(&servicio, motor.reglas, modo);
        let mut dto = HorasDelDiaDto {
            fecha,
            manana: Vec::new(),
            tarde: Vec::new(),
            noche: Vec::new(),
        };
        for (inicio, barbero_id) in motor.horas(fecha, &consulta) {
            let l = inicio.with_timezone(&ZONA_HORARIA);
            let hora = HoraLibreDto {
                inicio: local(inicio),
                hora: l.format("%H:%M").to_string(),
                barbero_id,
                barbero: motor.nombre(barbero_id),
            };
            match l.hour() {
                0..12 => dto.manana.push(hora),
                12..18 => dto.tarde.push(hora),
                _ => dto.noche.push(hora),
            }
        }
        Ok(dto)
    }
}
