//! Puertos del módulo, genéricos en el contexto transaccional `C` (ADR 0005).

use std::fmt;
use std::sync::Arc;

use archivos::api::Archivos;
use async_trait::async_trait;
use auditoria::api::{Auditor, EntradaAuditoria};
use catalogo::api::CatalogoServicios;
use chrono::{DateTime, NaiveDate, Utc};
use identidad::api::CuentasPersonal;
use negocio::api::ParametrosNegocio;
use nucleo::eventos::{BusEventos, EventoDominio};
use nucleo::identificadores::{
    IdArchivo, IdAusencia, IdBarbero, IdBloqueo, IdDocumento, IdServicio, IdUsuario,
};
use nucleo::{Actor, ErrorPersistencia, Porcentaje, Reloj, Transacciones};
use plataforma::seguridad::Cifrador;

use super::dto::{BarberoDto, CuentaDto, EstadoDto, HorarioDto, ServicioOfrecidoDto, VisibilidadDto};
use super::errores::ErrorBarberos;
use crate::dominio::{
    DatosBarbero, EstadoBarbero, Franja, Horario, NuevaAusencia, NuevoBloqueo, Retiro, TipoAdjunto,
};

/// Barbero leído de la base, con su estado del día.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BarberoGuardado {
    pub(crate) id: IdBarbero,
    pub(crate) usuario: IdUsuario,
    pub(crate) tipo_documento: String,
    pub(crate) numero_documento: String,
    pub(crate) nombres: String,
    pub(crate) apellidos: String,
    pub(crate) fecha_nacimiento: NaiveDate,
    pub(crate) celular: String,
    pub(crate) especialidad: String,
    pub(crate) anios_experiencia: Option<i16>,
    pub(crate) instagram: Option<String>,
    pub(crate) biografia: Option<String>,
    pub(crate) foto: Option<IdArchivo>,
    pub(crate) fecha_ingreso: NaiveDate,
    pub(crate) tipo_vinculacion: String,
    pub(crate) fin_contrato: Option<NaiveDate>,
    pub(crate) porcentaje_comision: Porcentaje,
    pub(crate) periodo_liquidacion: String,
    pub(crate) mostrar_en_landing: bool,
    pub(crate) recibe_citas_en_linea: bool,
    pub(crate) orden: i16,
    pub(crate) fecha_retiro: Option<NaiveDate>,
    pub(crate) motivo_retiro: Option<String>,
    pub(crate) estado: EstadoBarbero,
    pub(crate) estado_desde: Option<NaiveDate>,
    pub(crate) estado_hasta: Option<NaiveDate>,
    pub(crate) servicios: Vec<ServicioOfrecidoDto>,
}

impl BarberoGuardado {
    pub(crate) fn nombre(&self) -> String {
        format!("{} {}", self.nombres, self.apellidos)
    }

    /// Retirado con fecha ya cumplida, o con retiro programado.
    pub(crate) fn tiene_retiro(&self) -> bool {
        self.fecha_retiro.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AusenciaGuardada {
    pub(crate) id: IdAusencia,
    pub(crate) tipo: String,
    pub(crate) desde: NaiveDate,
    pub(crate) hasta: NaiveDate,
    pub(crate) observacion: Option<String>,
    pub(crate) soporte: Option<IdArchivo>,
    pub(crate) registrada_en: DateTime<Utc>,
    pub(crate) anulada: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BloqueoGuardado {
    pub(crate) id: IdBloqueo,
    pub(crate) inicio: DateTime<Utc>,
    pub(crate) fin: DateTime<Utc>,
    pub(crate) motivo: String,
    pub(crate) nota: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentoGuardado {
    pub(crate) id: IdDocumento,
    pub(crate) tipo: String,
    pub(crate) periodo: Option<NaiveDate>,
    pub(crate) archivo: IdArchivo,
    pub(crate) cargado_en: DateTime<Utc>,
}

/// Documento por registrar (el archivo ya está guardado en `archivos`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevoDocumento {
    pub(crate) tipo: TipoAdjunto,
    pub(crate) periodo: Option<NaiveDate>,
    pub(crate) archivo: IdArchivo,
    pub(crate) por: IdUsuario,
    pub(crate) ahora: DateTime<Utc>,
}

/// Datos privados tal como se guardan: lo sensible, cifrado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PrivadosGuardados {
    pub(crate) direccion: Option<String>,
    pub(crate) ciudad: Option<String>,
    pub(crate) grupo_sanguineo_cifrado: Option<Vec<u8>>,
    pub(crate) emergencia_nombre: Option<String>,
    pub(crate) emergencia_parentesco: Option<String>,
    pub(crate) emergencia_celular: Option<String>,
    pub(crate) eps: Option<String>,
    pub(crate) fondo_pensiones: Option<String>,
    pub(crate) arl: Option<String>,
    pub(crate) caja_compensacion: Option<String>,
    pub(crate) medio_pago: Option<String>,
    pub(crate) banco: Option<String>,
    pub(crate) tipo_cuenta: Option<String>,
    pub(crate) numero_cuenta_cifrado: Option<Vec<u8>>,
}

/// Restricciones de la base que el módulo traduce a errores de negocio.
pub(crate) const RESTRICCION_DOCUMENTO: &str = "barbero_documento_uk";
pub(crate) const RESTRICCION_AUSENCIA: &str = "ausencia_sin_solape";
pub(crate) const RESTRICCION_BLOQUEO: &str = "bloqueo_sin_solape";
pub(crate) const RESTRICCION_PLANILLA: &str = "documento_pila_uk";

#[async_trait]
pub(crate) trait RepositorioBarberos<C: Send>: Send + Sync + fmt::Debug {
    /// Equipo para la agenda: vigentes en `desde`, con franjas, servicios, ausencias y bloqueos
    /// que se cruzan con el rango. `foto_url` va vacía (la completa la fachada).
    async fn equipo_agenda(
        &self,
        cx: &mut C,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<(crate::api::BarberoAgenda, Option<IdArchivo>)>, ErrorPersistencia>;

    /// Todos, con su estado en `fecha`; los retirados al final.
    async fn listar(&self, cx: &mut C, fecha: NaiveDate) -> Result<Vec<BarberoGuardado>, ErrorPersistencia>;
    async fn obtener(
        &self,
        cx: &mut C,
        id: IdBarbero,
        fecha: NaiveDate,
    ) -> Result<Option<BarberoGuardado>, ErrorPersistencia>;
    /// Bloquea la fila hasta el fin de la transacción (cambios concurrentes del mismo barbero).
    async fn bloquear(&self, cx: &mut C, id: IdBarbero) -> Result<bool, ErrorPersistencia>;
    async fn nombres(
        &self,
        cx: &mut C,
        ids: &[IdBarbero],
    ) -> Result<std::collections::HashMap<IdBarbero, String>, ErrorPersistencia>;
    /// Lo agrega al final del orden del equipo.
    async fn crear(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
        datos: &DatosBarbero,
        mostrar_en_landing: bool,
        recibe_citas_en_linea: bool,
    ) -> Result<IdBarbero, ErrorPersistencia>;
    async fn actualizar(
        &self,
        cx: &mut C,
        id: IdBarbero,
        datos: &DatosBarbero,
    ) -> Result<(), ErrorPersistencia>;
    async fn cambiar_visibilidad(
        &self,
        cx: &mut C,
        id: IdBarbero,
        landing: bool,
        en_linea: bool,
    ) -> Result<(), ErrorPersistencia>;
    /// Barberos sin retiro, en su orden actual.
    async fn vigentes(&self, cx: &mut C) -> Result<Vec<IdBarbero>, ErrorPersistencia>;
    async fn reordenar(&self, cx: &mut C, ids: &[IdBarbero]) -> Result<(), ErrorPersistencia>;
    async fn retirar(&self, cx: &mut C, id: IdBarbero, retiro: &Retiro) -> Result<(), ErrorPersistencia>;
    /// Devuelve la foto anterior.
    async fn cambiar_foto(
        &self,
        cx: &mut C,
        id: IdBarbero,
        foto: IdArchivo,
    ) -> Result<Option<IdArchivo>, ErrorPersistencia>;
    async fn franjas(&self, cx: &mut C, id: IdBarbero) -> Result<Vec<Franja>, ErrorPersistencia>;
    async fn reemplazar_franjas(
        &self,
        cx: &mut C,
        id: IdBarbero,
        horario: &Horario,
    ) -> Result<(), ErrorPersistencia>;
    async fn reemplazar_servicios(
        &self,
        cx: &mut C,
        id: IdBarbero,
        ids: &[IdServicio],
    ) -> Result<(), ErrorPersistencia>;
}

#[async_trait]
pub(crate) trait RepositorioNovedades<C: Send>: Send + Sync + fmt::Debug {
    async fn ausencias(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
    ) -> Result<Vec<AusenciaGuardada>, ErrorPersistencia>;
    /// `Conflicto { restriccion: "ausencia_sin_solape" }` si se cruza con otra vigente.
    async fn registrar_ausencia(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        ausencia: &NuevaAusencia,
        por: IdUsuario,
        ahora: DateTime<Utc>,
    ) -> Result<IdAusencia, ErrorPersistencia>;
    /// Solo anula una ausencia vigente; devuelve `None` si no existe o ya estaba anulada.
    async fn anular_ausencia(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        id: IdAusencia,
        ahora: DateTime<Utc>,
    ) -> Result<Option<AusenciaGuardada>, ErrorPersistencia>;
    /// Devuelve la ausencia (con el soporte anterior) o `None` si no existe.
    async fn poner_soporte(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        id: IdAusencia,
        archivo: IdArchivo,
    ) -> Result<Option<AusenciaGuardada>, ErrorPersistencia>;
    /// Bloqueos que terminan después de `desde`.
    async fn bloqueos(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        desde: DateTime<Utc>,
    ) -> Result<Vec<BloqueoGuardado>, ErrorPersistencia>;
    /// `Conflicto { restriccion: "bloqueo_sin_solape" }` si se cruza con otro.
    async fn crear_bloqueo(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        bloqueo: &NuevoBloqueo,
        por: IdUsuario,
        ahora: DateTime<Utc>,
    ) -> Result<IdBloqueo, ErrorPersistencia>;
    async fn eliminar_bloqueo(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        id: IdBloqueo,
    ) -> Result<Option<BloqueoGuardado>, ErrorPersistencia>;
}

#[async_trait]
pub(crate) trait RepositorioExpediente<C: Send>: Send + Sync + fmt::Debug {
    async fn documentos(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
    ) -> Result<Vec<DocumentoGuardado>, ErrorPersistencia>;
    /// `Duplicado { restriccion: "documento_pila_uk" }` si ya hay planilla de ese mes.
    async fn agregar_documento(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        documento: &NuevoDocumento,
    ) -> Result<IdDocumento, ErrorPersistencia>;
    async fn documento(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        id: IdDocumento,
    ) -> Result<Option<DocumentoGuardado>, ErrorPersistencia>;
    async fn quitar_documento(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        id: IdDocumento,
    ) -> Result<Option<DocumentoGuardado>, ErrorPersistencia>;
    async fn datos_privados(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
    ) -> Result<Option<PrivadosGuardados>, ErrorPersistencia>;
    async fn guardar_privados(
        &self,
        cx: &mut C,
        barbero: IdBarbero,
        datos: &PrivadosGuardados,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
}

/// Lo que comparten los casos de uso del módulo.
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) barberos: Arc<dyn RepositorioBarberos<C>>,
    pub(crate) novedades: Arc<dyn RepositorioNovedades<C>>,
    pub(crate) expediente: Arc<dyn RepositorioExpediente<C>>,
    /// Del módulo `identidad`.
    pub(crate) cuentas: Arc<dyn CuentasPersonal<C>>,
    /// Del módulo `catalogo`.
    pub(crate) catalogo: Arc<dyn CatalogoServicios<C>>,
    /// Del módulo `archivos`.
    pub(crate) archivos: Arc<dyn Archivos<C>>,
    /// Del módulo `negocio`.
    pub(crate) parametros: Arc<dyn ParametrosNegocio<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) bus: Arc<BusEventos<C>>,
    pub(crate) cifrador: Cifrador,
    pub(crate) reloj: Arc<dyn Reloj>,
}

impl<C: Send> fmt::Debug for Puertos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PuertosBarberos").finish_non_exhaustive()
    }
}

pub(crate) const MODULO: &str = "barberos";
pub(crate) const ENTIDAD: &str = "barbero";

impl<C: Send> Puertos<C> {
    pub(crate) fn hoy(&self) -> NaiveDate {
        self.reloj.hoy()
    }

    pub(crate) async fn guardado(&self, cx: &mut C, id: IdBarbero) -> Result<BarberoGuardado, ErrorBarberos> {
        self.barberos
            .obtener(cx, id, self.hoy())
            .await?
            .ok_or(ErrorBarberos::NoEncontrado)
    }

    /// Ficha o fila del listado con la cuenta (de `identidad`), la foto (de `archivos`) y,
    /// si se pide, el horario.
    pub(crate) async fn dto(
        &self,
        cx: &mut C,
        g: BarberoGuardado,
        con_horario: bool,
    ) -> Result<BarberoDto, ErrorBarberos> {
        let cuenta = self.cuentas.cuenta(cx, g.usuario).await?;
        let foto_url = match g.foto {
            Some(foto) => self.archivos.urls_publicas(cx, &[foto]).await?.remove(&foto),
            None => None,
        };
        let horario = if con_horario {
            Some(HorarioDto::from(&Horario::reconstruir(
                self.barberos.franjas(cx, g.id).await?,
            )))
        } else {
            None
        };
        let nombre = g.nombre();
        Ok(BarberoDto {
            id: g.id,
            usuario_id: g.usuario,
            nombre,
            nombres: g.nombres,
            apellidos: g.apellidos,
            tipo_documento: g.tipo_documento,
            numero_documento: g.numero_documento,
            fecha_nacimiento: g.fecha_nacimiento,
            celular: g.celular,
            especialidad: g.especialidad,
            anios_experiencia: g.anios_experiencia,
            instagram: g.instagram,
            biografia: g.biografia,
            foto_url,
            fecha_ingreso: g.fecha_ingreso,
            tipo_vinculacion: g.tipo_vinculacion,
            fin_contrato: g.fin_contrato,
            porcentaje_comision: g.porcentaje_comision,
            periodo_liquidacion: g.periodo_liquidacion,
            estado: EstadoDto::nuevo(g.estado, g.estado_desde, g.estado_hasta),
            fecha_retiro: g.fecha_retiro,
            motivo_retiro: g.motivo_retiro,
            visibilidad: VisibilidadDto {
                mostrar_en_landing: g.mostrar_en_landing,
                recibe_citas_en_linea: g.recibe_citas_en_linea,
                acceso_panel: cuenta.as_ref().is_some_and(|c| c.acceso_panel),
            },
            cuenta: cuenta.map(|c| CuentaDto {
                correo: c.correo,
                pendiente_activacion: c.pendiente_activacion,
                ultimo_acceso_en: c.ultimo_acceso_en,
            }),
            orden: g.orden,
            servicios: g.servicios,
            horario,
        })
    }

    pub(crate) async fn ficha(&self, cx: &mut C, id: IdBarbero) -> Result<BarberoDto, ErrorBarberos> {
        let guardado = self.guardado(cx, id).await?;
        self.dto(cx, guardado, true).await
    }

    pub(crate) fn entrada(
        &self,
        actor: &Actor,
        accion: &'static str,
        entidad_id: impl fmt::Display,
    ) -> EntradaAuditoria {
        EntradaAuditoria::nueva(actor, MODULO, accion, ENTIDAD, entidad_id)
    }

    pub(crate) async fn auditar(&self, cx: &mut C, entrada: EntradaAuditoria) -> Result<(), ErrorBarberos> {
        Ok(self.auditor.registrar(cx, entrada).await?)
    }

    /// Servicios que puede ofrecer: deben existir; los que agrega, además, estar activos
    /// (RF-BAR-08). Los que ya ofrecía se pueden conservar aunque se hayan desactivado.
    pub(crate) async fn validar_servicios(
        &self,
        cx: &mut C,
        nuevos: &[IdServicio],
        actuales: &[IdServicio],
    ) -> Result<(), ErrorBarberos> {
        let resumenes = self.catalogo.resumenes(cx, nuevos).await?;
        let validos = nuevos.iter().all(|id| {
            resumenes
                .iter()
                .any(|s| s.id == *id && (s.activo || actuales.contains(id)))
        });
        if validos {
            Ok(())
        } else {
            Err(ErrorBarberos::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
                "servicios",
                "Elige servicios activos del catálogo.",
            )]))
        }
    }
}

impl<C: Send + 'static> Puertos<C> {
    pub(crate) async fn publicar<E: EventoDominio>(
        &self,
        cx: &mut C,
        evento: &E,
    ) -> Result<(), ErrorBarberos> {
        self.bus
            .publicar(cx, evento)
            .await
            .map_err(|e| ErrorBarberos::Interno(e.to_string()))
    }
}
