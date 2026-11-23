//! Registro, consulta y anulación de ventas (RF-VEN-01 a 10 y 12, RF-PBA-03 a 05).

use std::collections::HashMap;
use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use clientes::api::NuevoCliente;
use contratos::ventas::{VentaAnulada, VentaRegistrada};
use nucleo::identificadores::{IdBarbero, IdCita, IdCliente, IdServicio, IdUsuario, IdVenta};
use nucleo::{Actor, Dinero, ErrorPersistencia, Pagina, SolicitudPagina, Validacion, ZONA_HORARIA};
use serde_json::json;

use super::dto::{
    AnulacionDto, CatalogosDto, DescuentoAplicadoDto, DescuentoDto, LineaDto, MetodoPagoDto, MotivoDto,
    NombreDto, PagoDto, VentaDto,
};
use super::errores::ErrorVentas;
use super::puertos::{
    Descuento, FiltroVentas, MetodoPago, Motivo, NuevaVenta, Puertos, RESTRICCION_CITA_COBRADA, VentaGuardada,
};
use crate::dominio::{MAX_SERVICIOS, TipoDescuento, calcular, validar_pagos};

/// Cliente de la venta (RF-VEN-03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClienteVenta {
    Registrado(IdCliente),
    Nuevo(NuevoCliente),
    /// No suma a ningún historial (RN-16).
    Ocasional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SolicitudVenta {
    pub(crate) cita: Option<IdCita>,
    /// Con cita, el cliente es el de la cita.
    pub(crate) cliente: Option<ClienteVenta>,
    /// El barbero que lo registra queda fijo (RF-PBA-04); con cita, por defecto el de la cita.
    pub(crate) barbero: Option<IdBarbero>,
    pub(crate) servicios: Vec<IdServicio>,
    /// `(descuento, motivo)`.
    pub(crate) descuento: Option<(i16, i16)>,
    pub(crate) propina: i64,
    pub(crate) pagos: Vec<(i16, i64)>,
}

pub(crate) fn es_personal(actor: &Actor) -> Result<(), ErrorVentas> {
    match actor {
        Actor::Administrador { .. } | Actor::Barbero { .. } => Ok(()),
        _ => Err(ErrorVentas::Prohibido),
    }
}

pub(crate) fn solo_administrador(actor: &Actor) -> Result<IdUsuario, ErrorVentas> {
    match actor {
        Actor::Administrador { usuario } => Ok(*usuario),
        _ => Err(ErrorVentas::Prohibido),
    }
}

pub(crate) fn descuento_dto(d: &Descuento) -> DescuentoDto {
    let (tipo, valor) = match d.tipo {
        TipoDescuento::Porcentaje(p) => ("porcentaje", i64::from(p.centesimas() / 100)),
        TipoDescuento::ValorFijo(v) => ("valor_fijo", v.valor()),
    };
    DescuentoDto {
        id: d.id,
        nombre: d.nombre.clone(),
        tipo,
        valor,
        permitido_a_barberos: d.permitido_a_barberos,
    }
}

pub(crate) fn metodo_dto(m: &MetodoPago) -> MetodoPagoDto {
    MetodoPagoDto {
        id: m.id,
        codigo: m.codigo.clone(),
        nombre: m.nombre.clone(),
        activo: m.activo,
        orden: m.orden,
    }
}

pub(crate) struct Ventas<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Ventas<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ventas")
    }
}

impl<C: Send + 'static> Ventas<C> {
    /// Métodos de pago activos, descuentos (el barbero solo ve los permitidos a barberos) y
    /// motivos.
    pub(crate) async fn catalogos(&self, actor: &Actor) -> Result<CatalogosDto, ErrorVentas> {
        es_personal(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        let descuentos = p.ventas.descuentos(&mut cx).await?;
        let motivos = p.ventas.motivos(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        let barbero = matches!(actor, Actor::Barbero { .. });
        Ok(CatalogosDto {
            metodos_pago: metodos.iter().filter(|m| m.activo).map(metodo_dto).collect(),
            descuentos: descuentos
                .iter()
                .filter(|d| d.activo && (!barbero || d.permitido_a_barberos))
                .map(descuento_dto)
                .collect(),
            motivos_descuento: motivos
                .iter()
                .filter(|m| m.activo)
                .map(|m| MotivoDto {
                    id: m.id,
                    nombre: m.nombre.clone(),
                })
                .collect(),
            propinas_rapidas: [2_000, 5_000, 10_000],
        })
    }

    /// Registra la venta en una sola transacción (RNF-INT-02): número de ticket, líneas con el
    /// precio vigente, pagos, la cita «Atendida» y la visita en el historial del cliente.
    pub(crate) async fn registrar(&self, actor: &Actor, s: SolicitudVenta) -> Result<VentaDto, ErrorVentas> {
        es_personal(actor)?;
        let usuario = actor.usuario().ok_or(ErrorVentas::Prohibido)?;
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let hoy = p.reloj.hoy();
        let mut v = Validacion::default();
        if s.servicios.is_empty() || s.servicios.len() > MAX_SERVICIOS {
            v.error(
                "servicio_ids",
                format!("Agrega entre 1 y {MAX_SERVICIOS} servicios."),
            );
        }
        // Los montos se validan aquí, antes de convertirlos a `Dinero`: más adelante un valor
        // fuera de rango no puede volverse 0 en silencio.
        if s.propina < 0 {
            v.error("propina", "La propina no puede ser negativa.");
        } else if s.propina > crate::dominio::MAX_PROPINA {
            v.error("propina", "Revisa la propina: parece demasiado alta.");
        }
        if s.pagos.iter().any(|(_, monto)| *monto > Dinero::MAXIMO) {
            v.error("pagos", "Revisa los montos de los pagos: uno es demasiado alto.");
        }
        v.terminar(|| ())?;

        let mut cx = p.transacciones.iniciar().await?;
        // La cita (bloqueada: nadie la cancela ni la cobra mientras tanto).
        let cita = match s.cita {
            None => None,
            Some(id) => {
                let cita = p
                    .citas
                    .para_cobrar(&mut cx, id)
                    .await?
                    .filter(|c| !matches!(actor, Actor::Barbero { barbero, .. } if *barbero != c.barbero))
                    .ok_or_else(|| ErrorVentas::campo("cita_id", "La cita no existe."))?;
                if cita.estado == "atendida" {
                    return Err(ErrorVentas::CitaYaCobrada);
                }
                if !cita.por_atender {
                    return Err(ErrorVentas::EstadoInvalido("La cita ya no está por atender."));
                }
                if cita.inicio.with_timezone(&ZONA_HORARIA).date_naive() > hoy {
                    return Err(ErrorVentas::campo(
                        "cita_id",
                        "Solo se cobran citas de hoy o anteriores.",
                    ));
                }
                Some(cita)
            }
        };

        // El barbero: el de la sesión si es barbero (RF-PBA-04); si no, el indicado o el de la cita.
        let barbero_id = match actor {
            Actor::Barbero { barbero, .. } => *barbero,
            _ => s
                .barbero
                .or(cita.as_ref().map(|c| c.barbero))
                .ok_or_else(|| ErrorVentas::campo("barbero_id", "Elige el barbero que hizo el servicio."))?,
        };
        let barbero = p
            .equipo
            .para_venta(&mut cx, barbero_id, hoy)
            .await?
            .ok_or_else(|| ErrorVentas::campo("barbero_id", "El barbero no existe."))?;
        if !barbero.activo {
            return Err(ErrorVentas::campo("barbero_id", "El barbero no está activo hoy."));
        }

        // Servicios: activos y que el barbero ofrezca (RN-05), con el precio vigente (RN-18).
        let resumenes = p.catalogo.resumenes(&mut cx, &s.servicios).await?;
        let mut v = Validacion::default();
        let mut lineas = Vec::with_capacity(s.servicios.len());
        for id in &s.servicios {
            match resumenes.iter().find(|r| r.id == *id) {
                Some(r) if r.activo && barbero.servicios.contains(id) => lineas.push((*id, r.precio)),
                Some(r) if !r.activo => v.error("servicio_ids", format!("«{}» está inactivo.", r.nombre)),
                Some(r) => v.error(
                    "servicio_ids",
                    format!("{} no realiza «{}».", barbero.nombre, r.nombre),
                ),
                None => v.error("servicio_ids", format!("El servicio {} no existe.", id.0)),
            }
        }

        // Descuento con motivo (RN-13); el barbero, solo los permitidos a barberos.
        let mut descuento = None;
        if let Some((descuento_id, motivo_id)) = s.descuento {
            let descuentos = p.ventas.descuentos(&mut cx).await?;
            let motivos = p.ventas.motivos(&mut cx).await?;
            match descuentos.into_iter().find(|d| d.id == descuento_id && d.activo) {
                None => v.error("descuento", "El descuento no existe."),
                Some(d) if matches!(actor, Actor::Barbero { .. }) && !d.permitido_a_barberos => {
                    v.error("descuento", "Ese descuento solo lo aplica la administradora.");
                }
                Some(d) => descuento = Some(d),
            }
            if !motivos.iter().any(|m| m.id == motivo_id && m.activo) {
                v.error("descuento", "Elige el motivo del descuento.");
            }
        }

        // Pagos con métodos activos (RF-VEN-07).
        let metodos = p.ventas.metodos_pago(&mut cx).await?;
        for (metodo, _) in &s.pagos {
            if !metodos.iter().any(|m| m.id == *metodo && m.activo) {
                v.error("pagos", "Ese método de pago no está habilitado.");
            }
        }
        let pagos: Vec<(i16, Dinero)> = s
            .pagos
            .iter()
            .map(|(m, monto)| (*m, Dinero::pesos(*monto).unwrap_or(Dinero::CERO)))
            .collect();
        v.terminar(|| ())?;

        let precios: Vec<Dinero> = lineas.iter().map(|(_, precio)| *precio).collect();
        let propina = Dinero::pesos(s.propina).unwrap_or(Dinero::CERO);
        let totales = calcular(
            &precios,
            descuento.as_ref().map(|d| d.tipo),
            propina,
            barbero.porcentaje_comision,
        )?;
        validar_pagos(&pagos, totales.total)?;

        // Cliente: el de la cita; registrado, nuevo (se crea, RF-CLI-01) u ocasional.
        let cliente = match (&cita, s.cliente) {
            (Some(c), None) => Some(c.cliente),
            (Some(c), Some(ClienteVenta::Registrado(id))) if id == c.cliente => Some(c.cliente),
            (Some(_), Some(_)) => {
                return Err(ErrorVentas::campo(
                    "cliente",
                    "La venta de una cita es para el cliente de la cita.",
                ));
            }
            (None, Some(ClienteVenta::Registrado(id))) => {
                if p.clientes.por_ids(&mut cx, &[id]).await?.is_empty() {
                    return Err(ErrorVentas::campo("cliente", "El cliente no existe."));
                }
                Some(id)
            }
            (None, Some(ClienteVenta::Nuevo(nuevo))) => {
                Some(p.clientes.registrar_desde_panel(&mut cx, actor, nuevo).await?)
            }
            (None, Some(ClienteVenta::Ocasional) | None) => None,
        };

        let numero_ticket = p.ventas.siguiente_ticket(&mut cx).await?;
        let nueva = NuevaVenta {
            numero_ticket,
            registrada_en: ahora,
            registrada_por: usuario,
            barbero: barbero.id,
            cliente,
            cita: cita.as_ref().map(|c| c.id),
            descuento: descuento
                .as_ref()
                .zip(s.descuento)
                .map(|(d, (_, motivo))| (d.id, motivo, totales.descuento)),
            propina,
            porcentaje_comision: barbero.porcentaje_comision,
            lineas,
            pagos,
        };
        let id = match p.ventas.insertar(&mut cx, &nueva).await {
            Ok(id) => id,
            Err(ErrorPersistencia::Duplicado { restriccion }) if restriccion == RESTRICCION_CITA_COBRADA => {
                return Err(ErrorVentas::CitaYaCobrada);
            }
            Err(otro) => return Err(otro.into()),
        };
        p.bus
            .publicar(
                &mut cx,
                &VentaRegistrada {
                    venta: id,
                    registrada_por: usuario,
                    actor: actor.tipo(),
                    barbero: barbero.id,
                    cliente,
                    cita: nueva.cita,
                    venta_neta: totales.venta_neta,
                    fecha_operacion: hoy,
                },
            )
            .await?;
        let venta = self.guardada(&mut cx, id).await?;
        let dto = self.dtos(&mut cx, vec![venta]).await?.remove(0);
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    async fn guardada(&self, cx: &mut C, id: IdVenta) -> Result<VentaGuardada, ErrorVentas> {
        self.p
            .ventas
            .obtener(cx, id, false)
            .await?
            .ok_or(ErrorVentas::VentaNoEncontrada)
    }

    /// Ventas filtradas; el barbero solo ve las suyas (RN-26).
    pub(crate) async fn listar(
        &self,
        actor: &Actor,
        mut filtro: FiltroVentas,
        pagina: SolicitudPagina,
    ) -> Result<Pagina<VentaDto>, ErrorVentas> {
        es_personal(actor)?;
        if let Actor::Barbero { barbero, .. } = actor {
            filtro.barbero = Some(*barbero);
        }
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let (ventas, total) = p.ventas.listar(&mut cx, &filtro, Some(pagina)).await?;
        let dtos = self.dtos(&mut cx, ventas).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(Pagina::nueva(dtos, pagina, total))
    }

    /// Detalle con líneas y pagos; lo ajeno «no existe» para el barbero.
    pub(crate) async fn detalle(&self, actor: &Actor, id: IdVenta) -> Result<VentaDto, ErrorVentas> {
        es_personal(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let venta = self.guardada(&mut cx, id).await?;
        if matches!(actor, Actor::Barbero { barbero, .. } if *barbero != venta.barbero) {
            return Err(ErrorVentas::VentaNoEncontrada);
        }
        let dto = self.dtos(&mut cx, vec![venta]).await?.remove(0);
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    /// Anulación con motivo, solo la administradora (RF-VEN-12, RN-17): la venta conserva su
    /// número, sale de los totales y queda en la auditoría.
    pub(crate) async fn anular(
        &self,
        actor: &Actor,
        id: IdVenta,
        motivo: &str,
    ) -> Result<VentaDto, ErrorVentas> {
        let usuario = solo_administrador(actor)?;
        let motivo = motivo.trim();
        if !(5..=300).contains(&motivo.chars().count()) {
            return Err(ErrorVentas::campo(
                "motivo",
                "Explica el motivo (de 5 a 300 caracteres).",
            ));
        }
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let venta = p
            .ventas
            .obtener(&mut cx, id, true)
            .await?
            .ok_or(ErrorVentas::VentaNoEncontrada)?;
        if !venta.registrada() {
            return Err(ErrorVentas::EstadoInvalido("La venta ya está anulada."));
        }
        let antes = self.dtos(&mut cx, vec![venta.clone()]).await?.remove(0);
        p.ventas
            .anular(&mut cx, id, usuario, motivo, p.reloj.ahora())
            .await?;
        let ultima_visita_vigente = match venta.cliente {
            Some(cliente) => p.ventas.ultima_visita_vigente(&mut cx, cliente).await?,
            None => None,
        };
        p.bus
            .publicar(
                &mut cx,
                &VentaAnulada {
                    venta: id,
                    anulada_por: usuario,
                    barbero: venta.barbero,
                    cliente: venta.cliente,
                    cita: venta.cita,
                    venta_neta: antes.venta_neta,
                    fecha_operacion: venta.fecha_operacion,
                    ultima_visita_vigente,
                },
            )
            .await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, "ventas", "anular_venta", "venta", id.0)
                    .antes(&json!({ "estado": "registrada", "numero_ticket": antes.numero_ticket, "total_cobrado": antes.total_cobrado }))
                    .despues(&json!({ "estado": "anulada", "motivo": motivo })),
            )
            .await?;
        let anulada = self.guardada(&mut cx, id).await?;
        let dto = self.dtos(&mut cx, vec![anulada]).await?.remove(0);
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    /// Arma los DTO de varias ventas con los nombres de barberos, clientes, servicios,
    /// descuentos y métodos (pocas consultas para toda la lista).
    pub(crate) async fn dtos(
        &self,
        cx: &mut C,
        ventas: Vec<VentaGuardada>,
    ) -> Result<Vec<VentaDto>, ErrorVentas> {
        let p = &self.p;
        let mut barberos: Vec<IdBarbero> = ventas.iter().map(|v| v.barbero).collect();
        barberos.sort_by_key(|b| b.0);
        barberos.dedup();
        let mut clientes: Vec<IdCliente> = ventas.iter().filter_map(|v| v.cliente).collect();
        clientes.sort_by_key(|c| c.0);
        clientes.dedup();
        let mut servicios: Vec<IdServicio> =
            ventas.iter().flat_map(|v| v.lineas.iter().map(|l| l.1)).collect();
        servicios.sort_by_key(|s| s.0);
        servicios.dedup();
        let nombres_barberos = p.equipo.nombres(cx, &barberos).await?;
        let nombres_clientes: HashMap<IdCliente, String> = p
            .clientes
            .por_ids(cx, &clientes)
            .await?
            .into_iter()
            .map(|c| (c.id, c.nombre))
            .collect();
        let nombres_servicios: HashMap<IdServicio, String> = p
            .catalogo
            .resumenes(cx, &servicios)
            .await?
            .into_iter()
            .map(|s| (s.id, s.nombre))
            .collect();
        let descuentos: Vec<Descuento> = p.ventas.descuentos(cx).await?;
        let motivos: Vec<Motivo> = p.ventas.motivos(cx).await?;
        let metodos: Vec<MetodoPago> = p.ventas.metodos_pago(cx).await?;
        Ok(ventas
            .into_iter()
            .map(|v| {
                let subtotal = v
                    .lineas
                    .iter()
                    .try_fold(Dinero::CERO, |s, l| s.sumar(l.2))
                    .unwrap_or(Dinero::CERO);
                let valor_descuento = v.descuento.map_or(Dinero::CERO, |d| d.2);
                let venta_neta = subtotal.restar(valor_descuento).unwrap_or(Dinero::CERO);
                let local = |instante: chrono::DateTime<chrono::Utc>| {
                    instante.with_timezone(&ZONA_HORARIA).fixed_offset()
                };
                VentaDto {
                    id: v.id.0,
                    numero_ticket: v.numero_ticket,
                    registrada_en: local(v.registrada_en),
                    fecha_operacion: v.fecha_operacion,
                    registrada_por: v.registrada_por.0,
                    estado: v.estado.clone(),
                    barbero: NombreDto {
                        id: v.barbero.0,
                        nombre: nombres_barberos.get(&v.barbero).cloned().unwrap_or_default(),
                    },
                    cliente: v.cliente.map(|c| NombreDto {
                        id: c.0,
                        nombre: nombres_clientes
                            .get(&c)
                            .cloned()
                            .unwrap_or_else(|| "Cliente anonimizado".to_owned()),
                    }),
                    cita_id: v.cita.map(|c| c.0),
                    lineas: v
                        .lineas
                        .iter()
                        .map(|(posicion, servicio, precio)| LineaDto {
                            posicion: *posicion,
                            servicio_id: servicio.0,
                            nombre: nombres_servicios.get(servicio).cloned().unwrap_or_default(),
                            precio: *precio,
                        })
                        .collect(),
                    subtotal,
                    descuento: v
                        .descuento
                        .map(|(descuento, motivo, valor)| DescuentoAplicadoDto {
                            descuento_id: descuento,
                            nombre: descuentos
                                .iter()
                                .find(|d| d.id == descuento)
                                .map(|d| d.nombre.clone())
                                .unwrap_or_default(),
                            motivo_descuento_id: motivo,
                            motivo: motivos
                                .iter()
                                .find(|m| m.id == motivo)
                                .map(|m| m.nombre.clone())
                                .unwrap_or_default(),
                            valor,
                        }),
                    venta_neta,
                    propina: v.propina,
                    total_cobrado: venta_neta.sumar(v.propina).unwrap_or(venta_neta),
                    porcentaje_comision: v.porcentaje_comision,
                    comision_barbero: v.porcentaje_comision.aplicar(venta_neta),
                    pagos: v
                        .pagos
                        .iter()
                        .map(|(metodo, monto)| PagoDto {
                            metodo_pago_id: *metodo,
                            nombre: metodos
                                .iter()
                                .find(|m| m.id == *metodo)
                                .map(|m| m.nombre.clone())
                                .unwrap_or_default(),
                            monto: *monto,
                        })
                        .collect(),
                    anulacion: v.anulacion.as_ref().map(|(en, por, motivo)| AnulacionDto {
                        anulada_en: local(*en),
                        anulada_por: por.0,
                        motivo: motivo.clone(),
                    }),
                }
            })
            .collect())
    }
}
