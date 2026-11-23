use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::NaiveDate;
use nucleo::identificadores::{IdBarbero, IdCliente};
use nucleo::{Actor, Dinero, ErrorDeCampo, ErrorPersistencia, Pagina, SolicitudPagina};

use super::dto::{
    ClienteFichaDto, ClienteListadoDto, ClienteRapidoDto, ConfiguracionSegmentosDto, ListadoClientesDto,
};
use super::errores::ErrorClientes;
use super::puertos::{ClienteGuardado, Puertos, RESTRICCION_CELULAR};
use super::{ENTIDAD, MODULO};
use crate::dominio::{DatosCliente, UmbralesSegmentos, segmento};

fn solo_administrador(actor: &Actor) -> Result<(), ErrorClientes> {
    if actor.es_administrador() {
        Ok(())
    } else {
        Err(ErrorClientes::Prohibido)
    }
}

fn solo_personal(actor: &Actor) -> Result<(), ErrorClientes> {
    if actor.es_personal() {
        Ok(())
    } else {
        Err(ErrorClientes::Prohibido)
    }
}

pub(crate) fn celular_en_uso(error: ErrorPersistencia) -> ErrorClientes {
    match error {
        ErrorPersistencia::Duplicado { restriccion } if restriccion == RESTRICCION_CELULAR => {
            ErrorClientes::CelularEnUso
        }
        otro => otro.into(),
    }
}

fn listado(g: &ClienteGuardado, hoy: NaiveDate, u: UmbralesSegmentos) -> ClienteListadoDto {
    let c = &g.conocido;
    ClienteListadoDto {
        id: c.id,
        nombre: c.nombre.clone(),
        celular: c.celular.clone(),
        correo: c.correo.clone(),
        visitas: c.visitas,
        total_gastado: Dinero::pesos(g.total_gastado).unwrap_or_default(),
        ultima_visita: g.ultima_visita,
        inasistencias: c.inasistencias,
        barbero_habitual_id: g.barbero_habitual.map(IdBarbero),
        segmento: segmento(c.visitas, g.ultima_visita, hoy, u),
    }
}

/// Datos que llegan del panel al crear o editar.
#[derive(Debug, Clone, Default)]
pub(crate) struct SolicitudCliente {
    pub(crate) nombre: String,
    pub(crate) celular: String,
    pub(crate) correo: Option<String>,
    pub(crate) fecha_nacimiento: Option<NaiveDate>,
    pub(crate) preferencias: Option<String>,
    pub(crate) recordatorios_activos: Option<bool>,
}

/// Búsqueda rápida para la agenda y la venta (RF-VEN-03): personal, máximo 8 resultados.
#[derive(Debug)]
pub(crate) struct BuscarClientes<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> BuscarClientes<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        texto: &str,
    ) -> Result<Vec<ClienteRapidoDto>, ErrorClientes> {
        solo_personal(actor)?;
        let texto = texto.trim();
        if texto.chars().count() < 2 {
            return Err(ErrorClientes::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "q",
                "Escribe al menos 2 caracteres.",
            )]));
        }
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let u = p.clientes.umbrales(&mut cx).await?;
        let (clientes, _) = p
            .clientes
            .buscar(
                &mut cx,
                Some(texto),
                Some(SolicitudPagina {
                    pagina: 1,
                    por_pagina: 8,
                }),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        let hoy = p.reloj.hoy();
        Ok(clientes
            .iter()
            .map(|g| ClienteRapidoDto {
                id: g.conocido.id,
                nombre: g.conocido.nombre.clone(),
                celular: g.conocido.celular.clone(),
                segmento: segmento(g.conocido.visitas, g.ultima_visita, hoy, u),
            })
            .collect())
    }
}

/// Listado paginado con métricas y segmento, y los indicadores de la base en `meta`
/// (RF-CLI-02, RF-CLI-04). Solo administradores.
#[derive(Debug)]
pub(crate) struct ListarClientes<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> ListarClientes<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        texto: Option<String>,
        segmento_buscado: Option<String>,
        pagina: SolicitudPagina,
    ) -> Result<ListadoClientesDto, ErrorClientes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let pagina = pagina.normalizada();
        let mut cx = p.transacciones.iniciar().await?;
        let u = p.clientes.umbrales(&mut cx).await?;
        let hoy = p.reloj.hoy();
        let texto = texto.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let resultado = match segmento_buscado.as_deref().filter(|s| !s.is_empty()) {
            // El segmento se calcula: con filtro se recorre la base filtrada y se pagina aquí.
            Some(buscado) => {
                let (todos, _) = p.clientes.buscar(&mut cx, texto, None).await?;
                let filtrados: Vec<ClienteListadoDto> = todos
                    .iter()
                    .map(|g| listado(g, hoy, u))
                    .filter(|c| c.segmento == buscado)
                    .collect();
                let total = i64::try_from(filtrados.len()).unwrap_or_default();
                let datos = filtrados
                    .into_iter()
                    .skip(usize::try_from(pagina.desplazamiento()).unwrap_or_default())
                    .take(usize::try_from(pagina.limite()).unwrap_or_default())
                    .collect();
                Pagina::nueva(datos, pagina, total)
            }
            None => {
                let (clientes, total) = p.clientes.buscar(&mut cx, texto, Some(pagina)).await?;
                Pagina::nueva(
                    clientes.iter().map(|g| listado(g, hoy, u)).collect(),
                    pagina,
                    total,
                )
            }
        };
        let meta = p.clientes.indicadores(&mut cx, hoy, u).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ListadoClientesDto {
            pagina: resultado,
            meta,
        })
    }
}

fn configuracion_dto(u: UmbralesSegmentos) -> ConfiguracionSegmentosDto {
    ConfiguracionSegmentosDto {
        max_visitas_nuevo: u.max_visitas_nuevo,
        max_visitas_recurrente: u.max_visitas_recurrente,
        dias_inactividad: u.dias_inactividad,
    }
}

/// Umbrales de los segmentos (RF-CFG-08). Solo administradores; cada cambio queda en la
/// auditoría. Los segmentos se calculan al consultar (RF-SIS-06): el cambio se ve de inmediato
/// en el listado, la ficha y los reportes («por reactivar» usa los días de inactividad).
#[derive(Debug)]
pub(crate) struct ConfiguracionSegmentos<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> ConfiguracionSegmentos<C> {
    pub(crate) async fn consultar(&self, actor: &Actor) -> Result<ConfiguracionSegmentosDto, ErrorClientes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let u = p.clientes.umbrales(&mut cx).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(configuracion_dto(u))
    }

    pub(crate) async fn guardar(
        &self,
        actor: &Actor,
        max_visitas_nuevo: i64,
        max_visitas_recurrente: i64,
        dias_inactividad: i64,
    ) -> Result<ConfiguracionSegmentosDto, ErrorClientes> {
        solo_administrador(actor)?;
        let nuevos = UmbralesSegmentos::nuevos(max_visitas_nuevo, max_visitas_recurrente, dias_inactividad)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let antes = p.clientes.umbrales(&mut cx).await?;
        p.clientes.guardar_umbrales(&mut cx, &nuevos).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "cambiar_segmentos", "configuracion_segmentos", 1)
                    .antes(&configuracion_dto(antes))
                    .despues(&configuracion_dto(nuevos)),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(configuracion_dto(nuevos))
    }
}

/// Ficha del cliente (RF-CLI-03). Solo administradores.
#[derive(Debug)]
pub(crate) struct FichaCliente<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> FichaCliente<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdCliente,
    ) -> Result<ClienteFichaDto, ErrorClientes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let ficha = ficha(p, &mut cx, id).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ficha)
    }
}

async fn ficha<C: Send>(p: &Puertos<C>, cx: &mut C, id: IdCliente) -> Result<ClienteFichaDto, ErrorClientes> {
    let g = p
        .clientes
        .por_ids(cx, &[id])
        .await?
        .into_iter()
        .next()
        .ok_or(ErrorClientes::NoEncontrado)?;
    let u = p.clientes.umbrales(cx).await?;
    let autorizaciones = p.clientes.autorizaciones(cx, id).await?;
    Ok(ClienteFichaDto {
        resumen: listado(&g, p.reloj.hoy(), u),
        fecha_nacimiento: g.conocido.fecha_nacimiento,
        preferencias: g.preferencias.clone(),
        recordatorios_activos: g.conocido.recordatorios_activos,
        cliente_desde: g.creado_en,
        ticket_promedio: (g.conocido.visitas > 0)
            .then(|| {
                let total = g.total_gastado;
                let visitas = i64::from(g.conocido.visitas);
                nucleo::Dinero::pesos((total + visitas / 2) / visitas).ok()
            })
            .flatten(),
        autorizaciones,
    })
}

/// Registro desde el panel (RF-CLI-01, RF-AGE-05): administrador o barbero.
#[derive(Debug)]
pub(crate) struct CrearCliente<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> CrearCliente<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        s: SolicitudCliente,
    ) -> Result<ClienteFichaDto, ErrorClientes> {
        solo_personal(actor)?;
        let p = &self.p;
        let datos = DatosCliente::nuevos(
            &s.nombre,
            &s.celular,
            s.correo.as_deref(),
            s.fecha_nacimiento,
            p.reloj.hoy(),
        )?;
        let mut cx = p.transacciones.iniciar().await?;
        let id = p.clientes.crear(&mut cx, &datos).await.map_err(celular_en_uso)?;
        let dto = ficha(p, &mut cx, id).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "crear_cliente", ENTIDAD, id).despues(&dto.resumen),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }
}

/// Edición de datos y preferencias (RF-CLI-07). Solo administradores; queda auditada.
#[derive(Debug)]
pub(crate) struct EditarCliente<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> EditarCliente<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdCliente,
        s: SolicitudCliente,
    ) -> Result<ClienteFichaDto, ErrorClientes> {
        solo_administrador(actor)?;
        let p = &self.p;
        let datos = DatosCliente::nuevos(
            &s.nombre,
            &s.celular,
            s.correo.as_deref(),
            s.fecha_nacimiento,
            p.reloj.hoy(),
        )?;
        let preferencias = s.preferencias.as_deref().map(str::trim).filter(|t| !t.is_empty());
        if preferencias.is_some_and(|t| t.chars().count() > 500) {
            return Err(ErrorClientes::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "preferencias",
                "Máximo 500 caracteres.",
            )]));
        }
        let mut cx = p.transacciones.iniciar().await?;
        let antes = ficha(p, &mut cx, id).await?;
        let recordatorios = s.recordatorios_activos.unwrap_or(antes.recordatorios_activos);
        p.clientes
            .actualizar(&mut cx, id, &datos, preferencias, recordatorios)
            .await
            .map_err(celular_en_uso)?;
        let despues = ficha(p, &mut cx, id).await?;
        p.auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "editar_cliente", ENTIDAD, id)
                    .antes(&antes)
                    .despues(&despues),
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
