//! Derechos del titular de los datos (Ley 1581, RNF-LEG-03, RF-CLI-07): anonimización a
//! petición del cliente y registro de sus solicitudes (consulta, actualización o supresión).

use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::{Datelike, Days, NaiveDate, Weekday};
use nucleo::identificadores::IdCliente;
use nucleo::{Actor, Pagina, SolicitudPagina, ZONA_HORARIA};
use serde_json::json;

use super::dto::SolicitudTitularDto;
use super::errores::ErrorClientes;
use super::puertos::{Puertos, SolicitudGuardada};
use super::{ENTIDAD, MODULO};

fn administrador(actor: &Actor) -> Result<i64, ErrorClientes> {
    match actor {
        Actor::Administrador { usuario } => Ok(usuario.0),
        _ => Err(ErrorClientes::Prohibido),
    }
}

fn texto(campo: &str, valor: &str, maximo: usize) -> Result<String, ErrorClientes> {
    let valor = valor.trim();
    if (5..=maximo).contains(&valor.chars().count()) {
        Ok(valor.to_owned())
    } else {
        Err(ErrorClientes::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
            campo,
            format!("Escribe entre 5 y {maximo} caracteres."),
        )]))
    }
}

/// Suma días hábiles (lunes a viernes).
pub(crate) fn sumar_dias_habiles(desde: NaiveDate, dias: u32) -> NaiveDate {
    let mut fecha = desde;
    let mut faltan = dias;
    while faltan > 0 {
        fecha = fecha.checked_add_days(Days::new(1)).unwrap_or(fecha);
        if !matches!(fecha.weekday(), Weekday::Sat | Weekday::Sun) {
            faltan -= 1;
        }
    }
    fecha
}

fn dto(s: SolicitudGuardada) -> SolicitudTitularDto {
    let plazo = if s.tipo == "consulta" { 10 } else { 15 };
    SolicitudTitularDto {
        id: s.id,
        cliente_id: s.cliente,
        responder_antes_de: sumar_dias_habiles(
            s.recibida_en.with_timezone(&ZONA_HORARIA).date_naive(),
            plazo,
        ),
        tipo: s.tipo,
        descripcion: s.descripcion,
        recibida_en: s.recibida_en,
        respondida_en: s.respondida_en,
        respondida_por: s.respondida_por,
        respuesta: s.respuesta,
    }
}

pub(crate) struct Titular<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Titular<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Titular")
    }
}

impl<C: Send> Titular<C> {
    /// Supresión a petición del titular (RF-CLI-07, E2E-CLI-04): borra nombre, celular, correo,
    /// cumpleaños y preferencias; el cliente desaparece de las búsquedas y los reportes
    /// conservan sus cifras. Si tiene citas por atender, hay que cancelarlas antes.
    pub(crate) async fn anonimizar(
        &self,
        actor: &Actor,
        id: IdCliente,
        motivo: &str,
    ) -> Result<(), ErrorClientes> {
        administrador(actor)?;
        let motivo = texto("motivo", motivo, 300)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if p.clientes.por_ids(&mut cx, &[id]).await?.is_empty() {
            return Err(ErrorClientes::NoEncontrado);
        }
        let pendientes = p.agenda.citas_pendientes(&mut cx, id).await?;
        if pendientes > 0 {
            return Err(ErrorClientes::CitasPendientes(pendientes));
        }
        if !p.titular.anonimizar(&mut cx, id, p.reloj.ahora()).await? {
            return Err(ErrorClientes::NoEncontrado);
        }
        // La auditoría no guarda datos personales: solo que ocurrió y por qué.
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "anonimizar_cliente", ENTIDAD, id.0)
                    .despues(&json!({ "anonimizado": true, "motivo": motivo })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }

    pub(crate) async fn solicitudes(
        &self,
        actor: &Actor,
        solo_abiertas: bool,
        pagina: SolicitudPagina,
    ) -> Result<Pagina<SolicitudTitularDto>, ErrorClientes> {
        administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let (filas, total) = p.titular.solicitudes(&mut cx, solo_abiertas, pagina).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(Pagina::nueva(filas.into_iter().map(dto).collect(), pagina, total))
    }

    /// Registra una solicitud recibida por cualquier canal (correo, WhatsApp, en persona).
    pub(crate) async fn registrar(
        &self,
        actor: &Actor,
        cliente: Option<IdCliente>,
        tipo: &str,
        descripcion: &str,
    ) -> Result<SolicitudTitularDto, ErrorClientes> {
        administrador(actor)?;
        if !["consulta", "actualizacion", "supresion"].contains(&tipo) {
            return Err(ErrorClientes::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
                "tipo",
                "Elige consulta, actualizacion o supresion.",
            )]));
        }
        let descripcion = texto("descripcion", descripcion, 2000)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if let Some(id) = cliente
            && p.clientes.por_ids(&mut cx, &[id]).await?.is_empty()
        {
            return Err(ErrorClientes::NoEncontrado);
        }
        let solicitud = p
            .titular
            .crear_solicitud(&mut cx, cliente, tipo, &descripcion, p.reloj.ahora())
            .await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(
                    actor,
                    MODULO,
                    "registrar_solicitud_titular",
                    "solicitud_titular",
                    solicitud.id,
                )
                .despues(&json!({ "tipo": tipo, "cliente_id": cliente })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto(solicitud))
    }

    pub(crate) async fn responder(
        &self,
        actor: &Actor,
        id: i64,
        respuesta: &str,
    ) -> Result<SolicitudTitularDto, ErrorClientes> {
        let usuario = administrador(actor)?;
        let respuesta = texto("respuesta", respuesta, 2000)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let Some(actual) = p.titular.solicitud(&mut cx, id).await? else {
            return Err(ErrorClientes::NoEncontrado);
        };
        if actual.respondida_en.is_some() {
            return Err(ErrorClientes::EstadoInvalido("La solicitud ya fue respondida."));
        }
        let respondida = p
            .titular
            .responder_solicitud(&mut cx, id, usuario, &respuesta, p.reloj.ahora())
            .await?
            .ok_or(ErrorClientes::NoEncontrado)?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(
                    actor,
                    MODULO,
                    "responder_solicitud_titular",
                    "solicitud_titular",
                    id,
                )
                .despues(&json!({ "respondida": true })),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto(respondida))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_plazos_cuentan_dias_habiles() {
        // Viernes 20 de noviembre de 2026 + 10 días hábiles = viernes 4 de diciembre.
        let viernes = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        assert_eq!(
            sumar_dias_habiles(viernes, 10),
            NaiveDate::from_ymd_opt(2026, 12, 4).unwrap()
        );
        assert_eq!(
            sumar_dias_habiles(viernes, 1),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
            "salta el fin de semana"
        );
    }
}
