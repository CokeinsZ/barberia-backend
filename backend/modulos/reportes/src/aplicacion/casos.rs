//! Casos de uso de los reportes (RF-REP-01 a 06, RF-DAS-01 a 08, RF-PBA-06). Todos son de
//! lectura; la única escritura es la auditoría de cada PDF descargado (RF-SIS-07).

use std::collections::HashMap;
use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeDelta, Utc};
use negocio::api::TipoDiaEspecialDto;
use nucleo::identificadores::IdBarbero;
use nucleo::{Actor, ZONA_HORARIA};
use serde_json::json;

use super::dto::{
    AlertasDto, AusenteDto, CitaResumenDto, CitasPorEstadoDto, ClientesPeriodoDto, ComparacionDto,
    CumpleanosDto, DashboardDto, DiaVentasDto, HoraPicoDto, LiquidacionDto, MesClientesDto, PeriodoDto,
    PorReactivarDto, RendimientoBarberoDto, ReporteBarberoDto, ReporteClientesDto, ResumenDto, SemanaDto,
    TendenciasDto, TotalesDto,
};
use super::errores::ErrorReportes;
use super::pdf;
use super::puertos::{CitaVista, DocumentoPdf, Puertos, Sumas};
use crate::dominio::calculos::{
    TipoDia, franjas_del_dia, minutos, ocupacion, porcentaje, promedio, ticket_promedio, variacion,
};
use crate::dominio::formato::hora;
use crate::dominio::periodo::Periodo;

/// Cuántos clientes se listan como los de más visitas y como «por reactivar».
const DESTACADOS: i64 = 10;
const LISTA_REACTIVAR: i64 = 100;
const PROXIMAS: usize = 10;

/// Inicio del día en Bogotá, en UTC.
pub(crate) fn inicio_del_dia(fecha: NaiveDate) -> DateTime<Utc> {
    fecha
        .and_hms_opt(0, 0, 0)
        .and_then(|d| d.and_local_timezone(ZONA_HORARIA).earliest())
        .map_or_else(Utc::now, |d| d.with_timezone(&Utc))
}

/// Los nacidos un 29 de febrero celebran el 28 en los años no bisiestos.
fn celebra_el_29_de_febrero(dia: NaiveDate) -> bool {
    (dia.month(), dia.day()) == (2, 28) && NaiveDate::from_ymd_opt(dia.year(), 2, 29).is_none()
}

fn siguiente(fecha: NaiveDate) -> NaiveDate {
    fecha.checked_add_days(Days::new(1)).unwrap_or(fecha)
}

pub(crate) fn totales(s: Sumas) -> TotalesDto {
    TotalesDto {
        clientes_atendidos: s.ventas,
        servicios: s.servicios,
        subtotal: s.subtotal,
        descuentos: s.descuentos,
        ventas_netas: s.ventas_netas,
        propinas: s.propinas,
        total_cobrado: s.total_cobrado,
        comisiones: s.comisiones,
        ticket_promedio: ticket_promedio(s.ventas_netas, s.ventas),
    }
}

fn liquidacion(barbero_id: i64, nombre: String, s: Sumas) -> LiquidacionDto {
    LiquidacionDto {
        barbero_id,
        nombre,
        clientes_atendidos: s.ventas,
        servicios: s.servicios,
        ventas_netas: s.ventas_netas,
        comision: s.comisiones,
        propinas: s.propinas,
        total_a_pagar: s.comisiones + s.propinas,
    }
}

fn con_porcentajes(mut c: ClientesPeriodoDto) -> ClientesPeriodoDto {
    c.recurrentes = c.atendidos - c.nuevos;
    c.porcentaje_retorno = porcentaje(c.con_retorno, c.atendidos);
    c
}

fn cita_resumen(c: &CitaVista) -> CitaResumenDto {
    let local = c.inicio.with_timezone(&ZONA_HORARIA);
    CitaResumenDto {
        cita_id: c.id,
        inicio: local.fixed_offset(),
        hora: hora(local.time()),
        cliente: c
            .cliente
            .clone()
            .unwrap_or_else(|| "Cliente anonimizado".to_owned()),
        servicio: c.servicio.clone(),
        barbero_id: c.barbero_id,
        barbero: c.barbero.clone(),
        estado: c.estado.clone(),
    }
}

/// Qué PDF se pide (RF-REP-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SolicitudPdf {
    Resumen(Periodo),
    Barbero(IdBarbero, Periodo),
    Barberos(Periodo),
    Clientes(Periodo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchivoPdf {
    pub(crate) nombre: String,
    pub(crate) contenido: Vec<u8>,
}

pub(crate) struct Reportes<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Reportes<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reportes")
    }
}

fn solo_administrador(actor: &Actor) -> Result<(), ErrorReportes> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorReportes::Prohibido)
    }
}

impl<C: Send + 'static> Reportes<C> {
    pub(crate) fn hoy(&self) -> NaiveDate {
        self.p.reloj.hoy()
    }

    /// RF-REP-01: mes o rango (RN-22).
    pub(crate) fn periodo(
        &self,
        mes: Option<&str>,
        desde: Option<NaiveDate>,
        hasta: Option<NaiveDate>,
    ) -> Result<Periodo, ErrorReportes> {
        Ok(Periodo::desde_parametros(mes, desde, hasta, self.hoy())?)
    }

    /// Resumen de ventas y servicios (RF-REP-02).
    pub(crate) async fn resumen(&self, actor: &Actor, periodo: Periodo) -> Result<ResumenDto, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let dto = self.armar_resumen(&mut cx, periodo).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    async fn armar_resumen(&self, cx: &mut C, periodo: Periodo) -> Result<ResumenDto, ErrorReportes> {
        let l = &self.p.lecturas;
        let sumas = l.sumas(cx, periodo.desde, periodo.hasta, None).await?;
        let por_dia: HashMap<NaiveDate, i64> = l
            .netas_por_dia(cx, periodo.desde, periodo.hasta)
            .await?
            .into_iter()
            .collect();
        let por_semana = periodo
            .semanas()
            .into_iter()
            .map(|s| SemanaDto {
                desde: s.desde,
                hasta: s.hasta,
                ventas_netas: s
                    .desde
                    .iter_days()
                    .take_while(|d| *d <= s.hasta)
                    .map(|d| por_dia.get(&d).copied().unwrap_or(0))
                    .sum(),
            })
            .collect();
        Ok(ResumenDto {
            periodo: periodo.into(),
            totales: totales(sumas),
            por_semana,
            por_servicio: l.por_servicio(cx, periodo.desde, periodo.hasta, None).await?,
            por_metodo: l.por_metodo(cx, periodo.desde, periodo.hasta).await?,
            por_barbero: l
                .por_barbero(cx, periodo.desde, periodo.hasta)
                .await?
                .into_iter()
                .map(|f| liquidacion(f.barbero_id, f.nombre, f.sumas))
                .collect(),
        })
    }

    /// Reporte detallado y liquidación de un barbero (RF-REP-03). El barbero solo ve el suyo:
    /// el de otro «no existe» para él (RF-PBA-06, RN-26).
    pub(crate) async fn barbero(
        &self,
        actor: &Actor,
        id: IdBarbero,
        periodo: Periodo,
    ) -> Result<ReporteBarberoDto, ErrorReportes> {
        match actor {
            Actor::Administrador { .. } => {}
            Actor::Barbero { barbero, .. } if *barbero == id => {}
            Actor::Barbero { .. } => return Err(ErrorReportes::BarberoNoEncontrado),
            _ => return Err(ErrorReportes::Prohibido),
        }
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let dto = self.armar_barbero(&mut cx, id, periodo).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    async fn armar_barbero(
        &self,
        cx: &mut C,
        id: IdBarbero,
        periodo: Periodo,
    ) -> Result<ReporteBarberoDto, ErrorReportes> {
        let l = &self.p.lecturas;
        let barbero = l
            .barbero(cx, id)
            .await?
            .ok_or(ErrorReportes::BarberoNoEncontrado)?;
        let sumas = l.sumas(cx, periodo.desde, periodo.hasta, Some(id)).await?;
        Ok(ReporteBarberoDto {
            periodo: periodo.into(),
            totales: totales(sumas),
            clientes_distintos: l.clientes_distintos(cx, periodo.desde, periodo.hasta, id).await?,
            servicios: l.por_servicio(cx, periodo.desde, periodo.hasta, Some(id)).await?,
            liquidacion: liquidacion(barbero.id, barbero.nombre.clone(), sumas),
            ventas: l.ventas_de_barbero(cx, periodo.desde, periodo.hasta, id).await?,
            barbero,
        })
    }

    /// Todos los barberos del periodo, uno por sección (RF-REP-04).
    pub(crate) async fn barberos(
        &self,
        actor: &Actor,
        periodo: Periodo,
    ) -> Result<Vec<ReporteBarberoDto>, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let ids = p
            .lecturas
            .barberos_del_periodo(&mut cx, periodo.desde, periodo.hasta)
            .await?;
        let mut reportes = Vec::with_capacity(ids.len());
        for id in ids {
            reportes.push(self.armar_barbero(&mut cx, id, periodo).await?);
        }
        p.transacciones.confirmar(cx).await?;
        Ok(reportes)
    }

    /// Reporte de clientes (RF-REP-05).
    pub(crate) async fn clientes(
        &self,
        actor: &Actor,
        periodo: Periodo,
    ) -> Result<ReporteClientesDto, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let dto = self.armar_clientes(&mut cx, periodo).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    async fn armar_clientes(
        &self,
        cx: &mut C,
        periodo: Periodo,
    ) -> Result<ReporteClientesDto, ErrorReportes> {
        let p = &self.p;
        let l = &p.lecturas;
        let hoy = p.reloj.hoy();
        let dias_inactividad = p.clientes.dias_inactividad(cx).await?;
        let limite = hoy
            .checked_sub_days(Days::new(u64::try_from(dias_inactividad).unwrap_or(60)))
            .unwrap_or(hoy);
        let (lista, total) = l.por_reactivar(cx, limite, hoy, LISTA_REACTIVAR).await?;
        Ok(ReporteClientesDto {
            periodo: periodo.into(),
            clientes: con_porcentajes(l.clientes_del_periodo(cx, periodo.desde, periodo.hasta).await?),
            inasistencias: l
                .inasistencias(
                    cx,
                    inicio_del_dia(periodo.desde),
                    inicio_del_dia(siguiente(periodo.hasta)),
                )
                .await?,
            mas_visitas: l
                .mas_visitas(cx, periodo.desde, periodo.hasta, DESTACADOS)
                .await?,
            por_reactivar: PorReactivarDto {
                dias_inactividad,
                total,
                clientes: lista,
            },
        })
    }

    /// Dashboard del día (RF-DAS-01 a 04). Para hoy, «hasta la hora actual»; para un día
    /// pasado, el día completo.
    pub(crate) async fn dashboard(
        &self,
        actor: &Actor,
        fecha: NaiveDate,
    ) -> Result<DashboardDto, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let l = &p.lecturas;
        let ahora = p.reloj.ahora();
        let hoy = p.reloj.hoy();
        if fecha > hoy {
            return Err(ErrorReportes::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
                "fecha",
                "El dashboard es de hoy o de un día anterior.",
            )]));
        }
        let corte = if fecha == hoy {
            ahora
        } else {
            inicio_del_dia(siguiente(fecha))
        };
        let mut cx = p.transacciones.iniciar().await?;
        let sumas_hoy = l.sumas(&mut cx, fecha, fecha, None).await?;
        let hace_una_semana = fecha.checked_sub_days(Days::new(7)).unwrap_or(fecha);
        let sumas_antes = l
            .sumas_hasta(&mut cx, hace_una_semana, corte - TimeDelta::days(7))
            .await?;
        let desde_tendencia = fecha.checked_sub_days(Days::new(11)).unwrap_or(fecha);
        let por_dia: HashMap<NaiveDate, i64> = l
            .netas_por_dia(&mut cx, desde_tendencia, fecha)
            .await?
            .into_iter()
            .collect();
        let citas = l
            .citas(
                &mut cx,
                inicio_del_dia(fecha),
                inicio_del_dia(siguiente(fecha)),
                &[],
            )
            .await?;
        let equipo = p.equipo.equipo(&mut cx, fecha, fecha).await?;
        let especiales = p.calendario.dias_especiales(&mut cx, fecha, fecha).await?;
        let por_barbero: HashMap<i64, Sumas> = l
            .por_barbero(&mut cx, fecha, fecha)
            .await?
            .into_iter()
            .map(|f| (f.barbero_id, f.sumas))
            .collect();
        p.transacciones.confirmar(cx).await?;

        let mut estados = CitasPorEstadoDto::default();
        for c in &citas {
            match c.estado.as_str() {
                "atendida" => estados.atendidas += 1,
                "en_curso" => estados.en_curso += 1,
                "no_asistio" => estados.no_asistio += 1,
                "cancelada" => estados.canceladas += 1,
                _ if c.inicio >= ahora => estados.proximas += 1,
                _ => estados.sin_cerrar += 1,
            }
        }
        let proximas = citas
            .iter()
            .filter(|c| matches!(c.estado.as_str(), "sin_confirmar" | "confirmada") && c.inicio >= ahora)
            .take(PROXIMAS)
            .map(cita_resumen)
            .collect();

        let tipo = especiales.first().map_or(TipoDia::Normal, |d| match d.tipo {
            TipoDiaEspecialDto::Festivo => TipoDia::Festivo,
            TipoDiaEspecialDto::Cerrado => TipoDia::Cerrado,
            TipoDiaEspecialDto::CierreTemprano => {
                d.hora_cierre.map_or(TipoDia::Normal, TipoDia::CierreTemprano)
            }
        });
        let dia_semana = u8::try_from(fecha.weekday().number_from_monday()).unwrap_or(1);
        let local = |instante: DateTime<Utc>| instante.with_timezone(&ZONA_HORARIA);
        // Minutos del día de un intervalo (lo que cae fuera del día se recorta).
        let en_el_dia = |inicio: DateTime<Utc>, fin: DateTime<Utc>| -> Option<(u32, u32)> {
            let (i, f) = (local(inicio), local(fin));
            let desde = if i.date_naive() < fecha {
                0
            } else if i.date_naive() > fecha {
                return None;
            } else {
                minutos(i.time())
            };
            let hasta = if f.date_naive() > fecha {
                24 * 60
            } else if f.date_naive() < fecha {
                return None;
            } else {
                minutos(f.time())
            };
            (hasta > desde).then_some((desde, hasta))
        };
        let mut ausentes = Vec::new();
        let barberos = equipo
            .iter()
            .map(|b| {
                let ausencia = b.ausencias.iter().find(|a| a.desde <= fecha && fecha <= a.hasta);
                if let Some(a) = ausencia {
                    ausentes.push(AusenteDto {
                        barbero_id: b.id.0,
                        nombre: b.nombre.clone(),
                        tipo: a.tipo.clone(),
                        hasta: a.hasta,
                    });
                }
                let franjas = if ausencia.is_some() {
                    Vec::new()
                } else {
                    let todas: Vec<_> = b
                        .franjas
                        .iter()
                        .map(|f| (f.dia_semana, f.inicio, f.fin))
                        .collect();
                    franjas_del_dia(&todas, dia_semana, tipo)
                };
                let bloqueos: Vec<_> = b.bloqueos.iter().filter_map(|(i, f)| en_el_dia(*i, *f)).collect();
                let ocupadas: Vec<_> = citas
                    .iter()
                    .filter(|c| {
                        c.barbero_id == b.id.0
                            && matches!(
                                c.estado.as_str(),
                                "sin_confirmar" | "confirmada" | "en_curso" | "atendida"
                            )
                    })
                    .filter_map(|c| en_el_dia(c.inicio, c.fin))
                    .collect();
                let (ocupados, disponibles) = ocupacion(&franjas, &bloqueos, &ocupadas);
                let s = por_barbero.get(&b.id.0).copied().unwrap_or_default();
                RendimientoBarberoDto {
                    barbero_id: b.id.0,
                    nombre: b.nombre.clone(),
                    estado: ausencia.map_or_else(|| "activo".to_owned(), |a| a.tipo.clone()),
                    clientes_atendidos: s.ventas,
                    servicios: s.servicios,
                    ventas_netas: s.ventas_netas,
                    propinas: s.propinas,
                    minutos_ocupados: ocupados,
                    minutos_disponibles: disponibles,
                    ocupacion: porcentaje(i64::from(ocupados), i64::from(disponibles)),
                }
            })
            .collect();
        let hoy_totales = totales(sumas_hoy);
        let antes = totales(sumas_antes);
        Ok(DashboardDto {
            fecha,
            generado_en: local(ahora).fixed_offset(),
            hoy: hoy_totales,
            comparacion: ComparacionDto {
                fecha: hace_una_semana,
                variacion_ventas_netas: variacion(hoy_totales.ventas_netas, antes.ventas_netas),
                variacion_clientes: variacion(hoy_totales.clientes_atendidos, antes.clientes_atendidos),
                totales: antes,
            },
            tendencia: desde_tendencia
                .iter_days()
                .take_while(|d| *d <= fecha)
                .map(|d| DiaVentasDto {
                    fecha: d,
                    ventas_netas: por_dia.get(&d).copied().unwrap_or(0),
                })
                .collect(),
            citas: estados,
            proximas,
            barberos,
            totales_barberos: hoy_totales,
            ausentes,
        })
    }

    /// Tendencias del mes (RF-DAS-05, 06 y 07).
    pub(crate) async fn tendencias(
        &self,
        actor: &Actor,
        periodo: Periodo,
    ) -> Result<TendenciasDto, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let l = &p.lecturas;
        let mut cx = p.transacciones.iniciar().await?;
        let anio = periodo.hasta.year();
        let actual: HashMap<u32, i64> = l.atendidos_por_mes(&mut cx, anio).await?.into_iter().collect();
        let anterior: HashMap<u32, i64> = l
            .atendidos_por_mes(&mut cx, anio - 1)
            .await?
            .into_iter()
            .collect();
        let desde_picos = periodo
            .hasta
            .checked_sub_days(Days::new(27))
            .unwrap_or(periodo.hasta);
        let picos = l.ventas_por_hora(&mut cx, desde_picos, periodo.hasta).await?;
        let dto = TendenciasDto {
            por_servicio: l
                .por_servicio(&mut cx, periodo.desde, periodo.hasta, None)
                .await?,
            clientes: con_porcentajes(
                l.clientes_del_periodo(&mut cx, periodo.desde, periodo.hasta)
                    .await?,
            ),
            clientes_por_mes: (1..=12)
                .map(|mes| MesClientesDto {
                    mes,
                    anio_actual: actual.get(&mes).copied().unwrap_or(0),
                    anio_anterior: anterior.get(&mes).copied().unwrap_or(0),
                })
                .collect(),
            horas_pico: picos
                .into_iter()
                .map(|(dia_semana, hora, cantidad)| HoraPicoDto {
                    dia_semana,
                    hora,
                    // Promedio semanal de las últimas 4 semanas, con un decimal.
                    promedio: promedio(cantidad, 4),
                })
                .collect(),
            periodo: periodo.into(),
        };
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    /// Alertas (RF-DAS-08): citas de mañana sin confirmar, cumpleaños de la semana, barberos
    /// ausentes, clientes por reactivar, solicitudes del titular abiertas y avisos sin leer.
    pub(crate) async fn alertas(&self, actor: &Actor) -> Result<AlertasDto, ErrorReportes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let l = &p.lecturas;
        let hoy = p.reloj.hoy();
        let manana = siguiente(hoy);
        let semana: Vec<NaiveDate> = hoy.iter_days().take(7).collect();
        let mut dias: Vec<(u32, u32)> = semana.iter().map(|d| (d.month(), d.day())).collect();
        if semana.iter().any(|d| celebra_el_29_de_febrero(*d)) {
            dias.push((2, 29));
        }
        let mut cx = p.transacciones.iniciar().await?;
        let sin_confirmar = l
            .citas(
                &mut cx,
                inicio_del_dia(manana),
                inicio_del_dia(siguiente(manana)),
                &["sin_confirmar"],
            )
            .await?;
        let cumpleaneros = l.cumpleanos(&mut cx, &dias).await?;
        let equipo = p.equipo.equipo(&mut cx, hoy, hoy).await?;
        let dias_inactividad = p.clientes.dias_inactividad(&mut cx).await?;
        let limite = hoy
            .checked_sub_days(Days::new(u64::try_from(dias_inactividad).unwrap_or(60)))
            .unwrap_or(hoy);
        let por_reactivar = l.clientes_inactivos(&mut cx, limite).await?;
        let solicitudes_titular = p.clientes.solicitudes_abiertas(&mut cx).await?;
        let avisos_no_leidos = p.avisos.no_leidos(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        let mut cumpleanos: Vec<CumpleanosDto> = cumpleaneros
            .into_iter()
            .filter_map(|(id, nombre, celular, nacimiento)| {
                let fecha = semana.iter().copied().find(|d| {
                    (d.month(), d.day()) == (nacimiento.month(), nacimiento.day())
                        || ((nacimiento.month(), nacimiento.day()) == (2, 29) && celebra_el_29_de_febrero(*d))
                })?;
                Some(CumpleanosDto {
                    cliente_id: id,
                    nombre,
                    celular,
                    fecha,
                })
            })
            .collect();
        cumpleanos.sort_by(|a, b| a.fecha.cmp(&b.fecha).then(a.nombre.cmp(&b.nombre)));
        Ok(AlertasDto {
            sin_confirmar_manana: sin_confirmar.iter().map(cita_resumen).collect(),
            cumpleanos,
            ausentes: equipo
                .iter()
                .filter_map(|b| {
                    b.ausencias
                        .iter()
                        .find(|a| a.desde <= hoy && hoy <= a.hasta)
                        .map(|a| AusenteDto {
                            barbero_id: b.id.0,
                            nombre: b.nombre.clone(),
                            tipo: a.tipo.clone(),
                            hasta: a.hasta,
                        })
                })
                .collect(),
            por_reactivar,
            solicitudes_titular,
            avisos_no_leidos,
        })
    }

    /// PDF de un reporte (RF-REP-06, A4 con encabezado y pie numerado). Queda en la auditoría
    /// como exportación de datos (RF-SIS-07).
    ///
    /// Los permisos y los datos salen de los mismos casos de uso de la vista previa (un barbero
    /// recibe 403 o 404 aunque Gotenberg no esté). Mientras Gotenberg trabaja no se retiene
    /// ninguna conexión: la auditoría va en su propia transacción, solo si hubo PDF.
    pub(crate) async fn pdf(
        &self,
        actor: &Actor,
        solicitud: SolicitudPdf,
    ) -> Result<ArchivoPdf, ErrorReportes> {
        let p = &self.p;
        let (tipo, periodo, barbero) = match solicitud {
            SolicitudPdf::Resumen(periodo) => ("resumen", periodo, None),
            SolicitudPdf::Barbero(id, periodo) => ("barbero", periodo, Some(id)),
            SolicitudPdf::Barberos(periodo) => ("barberos", periodo, None),
            SolicitudPdf::Clientes(periodo) => ("clientes", periodo, None),
        };
        let (titulo, cuerpo, nombre) = match solicitud {
            SolicitudPdf::Resumen(periodo) => {
                let dto = self.resumen(actor, periodo).await?;
                (
                    "Resumen de ventas y servicios",
                    pdf::resumen(&dto),
                    format!("resumen-{}.pdf", periodo.para_archivo()),
                )
            }
            SolicitudPdf::Barbero(id, periodo) => {
                let dto = self.barbero(actor, id, periodo).await?;
                (
                    "Reporte detallado del barbero",
                    pdf::barberos(std::slice::from_ref(&dto)),
                    format!("barbero-{}-{}.pdf", id.0, periodo.para_archivo()),
                )
            }
            SolicitudPdf::Barberos(periodo) => {
                let dtos = self.barberos(actor, periodo).await?;
                (
                    "Reporte detallado de los barberos",
                    pdf::barberos(&dtos),
                    format!("barberos-{}.pdf", periodo.para_archivo()),
                )
            }
            SolicitudPdf::Clientes(periodo) => {
                let dto = self.clientes(actor, periodo).await?;
                (
                    "Reporte de clientes",
                    pdf::clientes(&dto),
                    format!("clientes-{}.pdf", periodo.para_archivo()),
                )
            }
        };
        let generador = p.pdf.clone().ok_or(ErrorReportes::PdfNoDisponible)?;
        let mut cx = p.transacciones.iniciar().await?;
        let negocio = p.negocio.identificacion(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        let generado = p.reloj.ahora().with_timezone(&ZONA_HORARIA);
        let documento = DocumentoPdf {
            cuerpo,
            encabezado: pdf::encabezado(
                &negocio.nombre_comercial,
                titulo,
                &PeriodoDto::from(periodo).texto,
                generado,
            ),
            pie: pdf::pie(&negocio.nombre_comercial, negocio.nit.as_deref()),
        };
        let contenido = generador.generar(&documento).await.map_err(|e| {
            tracing::warn!(error = %e, "no se pudo generar el PDF");
            ErrorReportes::PdfNoDisponible
        })?;
        let mut cx = p.transacciones.iniciar().await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, "reportes", "exportar_reporte", "reporte", tipo).despues(
                    &json!({
                        "tipo": tipo,
                        "desde": periodo.desde,
                        "hasta": periodo.hasta,
                        "barbero_id": barbero.map(|b| b.0),
                        "bytes": contenido.len(),
                    }),
                ),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ArchivoPdf { nombre, contenido })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn f(texto: &str) -> NaiveDate {
        texto.parse().unwrap()
    }

    #[test]
    fn el_29_de_febrero_se_celebra_el_28_solo_en_anios_no_bisiestos() {
        assert!(celebra_el_29_de_febrero(f("2027-02-28")));
        assert!(
            !celebra_el_29_de_febrero(f("2028-02-28")),
            "2028 es bisiesto: lo celebra el 29"
        );
        assert!(!celebra_el_29_de_febrero(f("2027-03-01")));
    }

    #[test]
    fn el_inicio_del_dia_es_medianoche_en_bogota() {
        assert_eq!(
            inicio_del_dia(f("2026-11-21")).to_rfc3339(),
            "2026-11-21T05:00:00+00:00"
        );
    }
}
