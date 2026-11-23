//! Caja del día, «Lo que llevas hoy», cierre de caja y métodos de pago (RF-VEN-10, RF-VEN-11,
//! RF-PBA-05, RF-CFG-07).

use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::NaiveDate;
use nucleo::identificadores::IdBarbero;
use nucleo::{Actor, Dinero, ErrorPersistencia, ZONA_HORARIA};
use serde_json::json;

use super::dto::{
    CajaDto, CierreDto, CierreMetodoDto, MetodoPagoDto, NombreDto, ResumenBarberoDto, TotalMetodoDto,
};
use super::errores::ErrorVentas;
use super::puertos::{CierreGuardado, FiltroVentas, MetodoPago, NuevoCierre, Puertos, RESTRICCION_CIERRE};
use super::ventas::{Ventas, metodo_dto, solo_administrador};

fn sumar(valores: impl Iterator<Item = Dinero>) -> Dinero {
    valores.fold(Dinero::CERO, |suma, v| suma.sumar(v).unwrap_or(suma))
}

pub(crate) struct Caja<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
    pub(crate) ventas: Ventas<C>,
}

impl<C: Send> std::fmt::Debug for Caja<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Caja")
    }
}

/// Contado por método al cerrar la caja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SolicitudCierre {
    pub(crate) fecha: NaiveDate,
    pub(crate) contado: Vec<(i16, i64)>,
    pub(crate) observacion: Option<String>,
}

impl<C: Send + 'static> Caja<C> {
    pub(crate) fn hoy(&self) -> NaiveDate {
        self.p.reloj.hoy()
    }

    /// Caja del día (RF-VEN-10): totales sin las anuladas y desglose por método.
    pub(crate) async fn caja(
        &self,
        actor: &Actor,
        fecha: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<CajaDto, ErrorVentas> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let filtro = FiltroVentas {
            desde: Some(fecha),
            hasta: Some(fecha),
            ..Default::default()
        };
        let (todas, _) = p.ventas.listar(&mut cx, &filtro, None).await?;
        let cobrado = p.ventas.cobrado_por_metodo(&mut cx, fecha).await?;
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        let dtos = self.ventas.dtos(&mut cx, todas).await?;
        p.transacciones.confirmar(cx).await?;
        let vigentes: Vec<_> = dtos.iter().filter(|v| v.estado == "registrada").collect();
        Ok(CajaDto {
            fecha,
            numero_ventas: vigentes.len(),
            subtotal: sumar(vigentes.iter().map(|v| v.subtotal)),
            descuentos: sumar(
                vigentes
                    .iter()
                    .map(|v| v.descuento.as_ref().map_or(Dinero::CERO, |d| d.valor)),
            ),
            venta_neta: sumar(vigentes.iter().map(|v| v.venta_neta)),
            propinas: sumar(vigentes.iter().map(|v| v.propina)),
            total_cobrado: sumar(vigentes.iter().map(|v| v.total_cobrado)),
            por_metodo: cobrado
                .into_iter()
                .map(|(id, total)| {
                    let metodo = metodos.iter().find(|m| m.id == id);
                    TotalMetodoDto {
                        metodo_pago_id: id,
                        codigo: metodo.map(|m| m.codigo.clone()).unwrap_or_default(),
                        nombre: metodo.map(|m| m.nombre.clone()).unwrap_or_default(),
                        total,
                    }
                })
                .collect(),
            anuladas: dtos.len() - vigentes.len(),
            ventas: dtos
                .iter()
                .filter(|v| barbero.is_none_or(|b| v.barbero.id == b.0))
                .cloned()
                .collect(),
        })
    }

    /// «Lo que llevas hoy» (RF-PBA-05): comisión + propinas y ventas netas del barbero. La
    /// administradora puede ver el de cualquier barbero.
    pub(crate) async fn resumen_barbero(
        &self,
        actor: &Actor,
        fecha: NaiveDate,
        barbero: Option<IdBarbero>,
    ) -> Result<ResumenBarberoDto, ErrorVentas> {
        let barbero = match actor {
            Actor::Barbero { barbero, .. } => *barbero,
            Actor::Administrador { .. } => {
                barbero.ok_or_else(|| ErrorVentas::campo("barbero_id", "Indica el barbero."))?
            }
            _ => return Err(ErrorVentas::Prohibido),
        };
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let filtro = FiltroVentas {
            desde: Some(fecha),
            hasta: Some(fecha),
            barbero: Some(barbero),
            solo_registradas: true,
            ..Default::default()
        };
        let (ventas, _) = p.ventas.listar(&mut cx, &filtro, None).await?;
        let nombre = p
            .equipo
            .nombres(&mut cx, &[barbero])
            .await?
            .remove(&barbero)
            .unwrap_or_default();
        let dtos = self.ventas.dtos(&mut cx, ventas).await?;
        p.transacciones.confirmar(cx).await?;
        let comision = sumar(dtos.iter().map(|v| v.comision_barbero));
        let propinas = sumar(dtos.iter().map(|v| v.propina));
        Ok(ResumenBarberoDto {
            fecha,
            barbero: NombreDto {
                id: barbero.0,
                nombre,
            },
            numero_ventas: dtos.len(),
            venta_neta: sumar(dtos.iter().map(|v| v.venta_neta)),
            comision,
            propinas,
            total_a_recibir: comision.sumar(propinas).unwrap_or(comision),
            ventas: dtos,
        })
    }

    pub(crate) async fn cierre(&self, actor: &Actor, fecha: NaiveDate) -> Result<CierreDto, ErrorVentas> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let cierre = p
            .ventas
            .cierre(&mut cx, fecha)
            .await?
            .ok_or(ErrorVentas::CierreNoEncontrado)?;
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(cierre_dto(cierre, &metodos))
    }

    /// Cierre de caja (RF-VEN-11): guarda lo esperado por método en ese momento (propinas
    /// incluidas) y lo contado; la diferencia se calcula al consultar. Uno por día (DA-09).
    pub(crate) async fn cerrar(&self, actor: &Actor, s: SolicitudCierre) -> Result<CierreDto, ErrorVentas> {
        let usuario = solo_administrador(actor)?;
        let p = &self.p;
        if s.fecha > p.reloj.hoy() {
            return Err(ErrorVentas::campo(
                "fecha",
                "No se puede cerrar la caja de un día que no ha llegado.",
            ));
        }
        let observacion = s
            .observacion
            .map(|o| o.trim().to_owned())
            .filter(|o| !o.is_empty());
        if observacion.as_ref().is_some_and(|o| o.chars().count() > 500) {
            return Err(ErrorVentas::campo("observacion", "Máximo 500 caracteres."));
        }
        let mut metodos_contados: Vec<i16> = s.contado.iter().map(|(m, _)| *m).collect();
        metodos_contados.sort_unstable();
        metodos_contados.dedup();
        if metodos_contados.len() != s.contado.len() || s.contado.iter().any(|(_, monto)| *monto < 0) {
            return Err(ErrorVentas::campo(
                "contado",
                "Un valor por método, sin negativos.",
            ));
        }
        // Antes de convertirlo a `Dinero`: un conteo fuera de rango no puede volverse 0 en silencio.
        if s.contado.iter().any(|(_, monto)| *monto > Dinero::MAXIMO) {
            return Err(ErrorVentas::campo(
                "contado",
                "Revisa lo contado: un valor es demasiado alto.",
            ));
        }
        let mut cx = p.transacciones.iniciar().await?;
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        if s.contado.iter().any(|(m, _)| !metodos.iter().any(|x| x.id == *m)) {
            return Err(ErrorVentas::campo("contado", "Ese método de pago no existe."));
        }
        let esperado = p.ventas.cobrado_por_metodo(&mut cx, s.fecha).await?;
        let mut ids: Vec<i16> = esperado.iter().map(|(m, _)| *m).chain(metodos_contados).collect();
        ids.sort_unstable();
        ids.dedup();
        let detalle = ids
            .into_iter()
            .map(|m| {
                let esperado = esperado
                    .iter()
                    .find(|(x, _)| *x == m)
                    .map_or(Dinero::CERO, |(_, v)| *v);
                let contado = s
                    .contado
                    .iter()
                    .find(|(x, _)| *x == m)
                    .map(|(_, v)| Dinero::pesos(*v).unwrap_or(Dinero::CERO));
                (m, esperado, contado)
            })
            .collect();
        let nuevo = NuevoCierre {
            fecha: s.fecha,
            cerrado_por: usuario,
            cerrado_en: p.reloj.ahora(),
            observacion,
            detalle,
        };
        let id = match p.ventas.crear_cierre(&mut cx, &nuevo).await {
            Ok(id) => id,
            Err(ErrorPersistencia::Duplicado { restriccion }) if restriccion == RESTRICCION_CIERRE => {
                return Err(ErrorVentas::CierreExistente);
            }
            Err(otro) => return Err(otro.into()),
        };
        let dto = cierre_dto(
            CierreGuardado {
                id,
                fecha: nuevo.fecha,
                cerrado_por: nuevo.cerrado_por,
                cerrado_en: nuevo.cerrado_en,
                observacion: nuevo.observacion.clone(),
                detalle: nuevo.detalle.clone(),
            },
            &metodos,
        );
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, "ventas", "cerrar_caja", "cierre_caja", id)
                    .despues(&json!({ "fecha": dto.fecha, "esperado": dto.total_esperado, "contado": dto.total_contado, "diferencia": dto.diferencia })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    /// Todos los métodos de pago, activos o no (RF-CFG-07).
    pub(crate) async fn metodos(&self, actor: &Actor) -> Result<Vec<MetodoPagoDto>, ErrorVentas> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(metodos.iter().map(metodo_dto).collect())
    }

    /// Habilita o deshabilita un método de pago (incluido «Link de pago»), auditado.
    pub(crate) async fn cambiar_metodo(
        &self,
        actor: &Actor,
        id: i16,
        activo: bool,
    ) -> Result<MetodoPagoDto, ErrorVentas> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes: Option<MetodoPago> = p
            .ventas
            .metodos_pago(&mut cx)
            .await?
            .into_iter()
            .find(|m| m.id == id);
        let antes = antes.ok_or(ErrorVentas::MetodoNoEncontrado)?;
        let metodo = p
            .ventas
            .cambiar_metodo(&mut cx, id, activo)
            .await?
            .ok_or(ErrorVentas::MetodoNoEncontrado)?;
        if !metodos_activos_con(&p.ventas.metodos_pago(&mut cx).await?) {
            return Err(ErrorVentas::EstadoInvalido(
                "Debe quedar al menos un método de pago activo.",
            ));
        }
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(
                    actor,
                    "ventas",
                    "cambiar_metodo_pago",
                    "metodo_pago",
                    i64::from(id),
                )
                .antes(&json!({ "activo": antes.activo }))
                .despues(&json!({ "activo": metodo.activo })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(metodo_dto(&metodo))
    }
}

fn metodos_activos_con(metodos: &[MetodoPago]) -> bool {
    metodos.iter().any(|m| m.activo)
}

fn cierre_dto(c: CierreGuardado, metodos: &[MetodoPago]) -> CierreDto {
    let filas: Vec<CierreMetodoDto> = c
        .detalle
        .iter()
        .map(|(id, esperado, contado)| CierreMetodoDto {
            metodo_pago_id: *id,
            nombre: metodos
                .iter()
                .find(|m| m.id == *id)
                .map(|m| m.nombre.clone())
                .unwrap_or_default(),
            esperado: *esperado,
            contado: *contado,
            diferencia: contado.map(|v| v.valor() - esperado.valor()),
        })
        .collect();
    let total_esperado = sumar(filas.iter().map(|f| f.esperado));
    let total_contado = sumar(filas.iter().filter_map(|f| f.contado));
    let diferencia = filas.iter().filter_map(|f| f.diferencia).sum();
    CierreDto {
        id: c.id,
        fecha: c.fecha,
        cerrado_por: c.cerrado_por.0,
        cerrado_en: c.cerrado_en.with_timezone(&ZONA_HORARIA).fixed_offset(),
        observacion: c.observacion,
        metodos: filas,
        total_esperado,
        total_contado,
        diferencia,
    }
}
