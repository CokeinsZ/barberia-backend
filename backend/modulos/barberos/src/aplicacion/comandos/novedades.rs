use std::time::Duration;

use archivos::api::{ArchivoNuevo, TipoArchivo, Visibilidad};
use chrono::{NaiveDate, NaiveTime, TimeDelta};
use contratos::barberos::{AusenciaRegistrada, BloqueoCreado};
use nucleo::identificadores::{IdAusencia, IdBarbero, IdBloqueo};
use nucleo::{Actor, ErrorPersistencia, ZONA_HORARIA};
use plataforma::http::ArchivoSubido;

use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::{AusenciaDto, BloqueoDto};
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::{administrador_o_propio, solo_administrador};
use crate::aplicacion::puertos::{
    AusenciaGuardada, BloqueoGuardado, Puertos, RESTRICCION_AUSENCIA, RESTRICCION_BLOQUEO,
};
use crate::dominio::{EstadoBarbero, NuevaAusencia, NuevoBloqueo};

const VIGENCIA_DESCARGA: Duration = Duration::from_secs(5 * 60);
const TAMANO_MAXIMO_SOPORTE: usize = 10 * 1024 * 1024;

fn ausencia_dto<C: Send>(p: &Puertos<C>, a: AusenciaGuardada) -> AusenciaDto {
    AusenciaDto {
        id: a.id,
        tipo: a.tipo,
        desde: a.desde,
        hasta: a.hasta,
        observacion: a.observacion,
        soporte_url: a.soporte.map(|s| p.archivos.url_firmada(s, VIGENCIA_DESCARGA)),
        registrada_en: a.registrada_en,
        anulada: a.anulada,
    }
}

fn bloqueo_dto(b: BloqueoGuardado) -> BloqueoDto {
    BloqueoDto::nuevo(b.id, b.inicio, b.fin, b.motivo, b.nota)
}

caso_de_uso!(
    /// Vacaciones, incapacidades, licencias y suspensiones (RF-BAR-11, RF-BAR-12). Solo las
    /// registra el administrador (RN-23). No cambian el estado guardado: el estado de cada día
    /// se calcula con ellas (RN-24).
    GestionAusencias
);

impl<C: Send + 'static> GestionAusencias<C> {
    pub(crate) async fn listar(
        &self,
        actor: &Actor,
        id: IdBarbero,
    ) -> Result<Vec<AusenciaDto>, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let ausencias = p.novedades.ausencias(&mut cx, id).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ausencias.into_iter().map(|a| ausencia_dto(p, a)).collect())
    }

    pub(crate) async fn registrar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        tipo: &str,
        desde: NaiveDate,
        hasta: NaiveDate,
        observacion: &str,
    ) -> Result<AusenciaDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let ausencia = NuevaAusencia::nueva(tipo, desde, hasta, observacion)?;
        let p = &self.p;
        let por = actor.usuario().ok_or(ErrorBarberos::Prohibido)?;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        if p.guardado(&mut cx, id).await?.estado == EstadoBarbero::Inactivo {
            return Err(ErrorBarberos::BarberoRetirado);
        }
        let ahora = p.reloj.ahora();
        let aid = match p
            .novedades
            .registrar_ausencia(&mut cx, id, &ausencia, por, ahora)
            .await
        {
            Ok(aid) => aid,
            Err(ErrorPersistencia::Conflicto { restriccion }) if restriccion == RESTRICCION_AUSENCIA => {
                return Err(ErrorBarberos::AusenciaSolapada);
            }
            Err(otro) => return Err(otro.into()),
        };
        p.publicar(
            &mut cx,
            &AusenciaRegistrada {
                barbero: id,
                desde: ausencia.desde,
                hasta: ausencia.hasta,
            },
        )
        .await?;
        let dto = AusenciaDto {
            id: aid,
            tipo: ausencia.tipo.codigo().to_owned(),
            desde: ausencia.desde,
            hasta: ausencia.hasta,
            observacion: Some(ausencia.observacion.clone()),
            soporte_url: None,
            registrada_en: ahora,
            anulada: false,
        };
        p.auditar(&mut cx, p.entrada(actor, "registrar_ausencia", id).despues(&dto))
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    pub(crate) async fn anular(
        &self,
        actor: &Actor,
        id: IdBarbero,
        ausencia: IdAusencia,
    ) -> Result<AusenciaDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let anulada = p
            .novedades
            .anular_ausencia(&mut cx, id, ausencia, p.reloj.ahora())
            .await?
            .ok_or(ErrorBarberos::RecursoNoEncontrado("La ausencia vigente"))?;
        let dto = ausencia_dto(p, anulada);
        p.auditar(&mut cx, p.entrada(actor, "anular_ausencia", id).despues(&dto))
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    /// Soporte de una incapacidad o licencia (RF-BAR-12): archivo privado.
    pub(crate) async fn adjuntar_soporte(
        &self,
        actor: &Actor,
        id: IdBarbero,
        ausencia: IdAusencia,
        archivo: Option<ArchivoSubido>,
    ) -> Result<AusenciaDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let archivo = archivo.ok_or(archivos::api::ErrorArchivos::Vacio { campo: "soporte" })?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let guardado = p
            .archivos
            .guardar(
                &mut cx,
                ArchivoNuevo {
                    campo: "soporte",
                    contenido: archivo.contenido,
                    nombre_original: archivo.nombre,
                    visibilidad: Visibilidad::Privado,
                    carpeta: "barberos/soportes",
                    tipos_permitidos: TipoArchivo::DOCUMENTOS,
                    tamano_maximo: TAMANO_MAXIMO_SOPORTE,
                    exigir_vertical: false,
                    subido_por: actor.usuario(),
                },
            )
            .await?;
        let anterior = p
            .novedades
            .poner_soporte(&mut cx, id, ausencia, guardado.id)
            .await?
            .ok_or(ErrorBarberos::RecursoNoEncontrado("La ausencia"))?;
        let clave_anterior = match anterior.soporte {
            Some(viejo) => p.archivos.eliminar(&mut cx, viejo).await?,
            None => None,
        };
        let dto = ausencia_dto(
            p,
            AusenciaGuardada {
                soporte: Some(guardado.id),
                ..anterior
            },
        );
        p.auditar(
            &mut cx,
            p.entrada(actor, "adjuntar_soporte_ausencia", id).despues(&dto.id),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        if let Some(clave) = clave_anterior {
            p.archivos.borrar_contenido(&clave).await;
        }
        Ok(dto)
    }
}

/// Fecha, horas y motivo de un bloqueo, tal como llegan.
#[derive(Debug, Clone)]
pub(crate) struct SolicitudBloqueo {
    pub(crate) fecha: NaiveDate,
    pub(crate) desde: NaiveTime,
    pub(crate) hasta: NaiveTime,
    pub(crate) motivo: String,
    pub(crate) nota: Option<String>,
}

caso_de_uso!(
    /// Bloqueos puntuales (RF-PBA-10): el barbero o el administrador retiran horas del
    /// agendamiento. Se publica `BloqueoCreado` para avisar de citas afectadas.
    GestionBloqueos
);

impl<C: Send + 'static> GestionBloqueos<C> {
    /// Bloqueos que aún no terminan desde `desde` (por defecto, hoy).
    pub(crate) async fn listar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        desde: Option<NaiveDate>,
    ) -> Result<Vec<BloqueoDto>, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let p = &self.p;
        let inicio_del_dia = desde
            .unwrap_or_else(|| p.hoy())
            .and_time(NaiveTime::MIN)
            .and_local_timezone(ZONA_HORARIA)
            .earliest()
            .map_or_else(|| p.reloj.ahora() - TimeDelta::days(1), |d| d.to_utc());
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let bloqueos = p.novedades.bloqueos(&mut cx, id, inicio_del_dia).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(bloqueos.into_iter().map(bloqueo_dto).collect())
    }

    pub(crate) async fn crear(
        &self,
        actor: &Actor,
        id: IdBarbero,
        s: SolicitudBloqueo,
    ) -> Result<BloqueoDto, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let bloqueo = NuevoBloqueo::nuevo(s.fecha, s.desde, s.hasta, &s.motivo, s.nota, ahora)?;
        let por = actor.usuario().ok_or(ErrorBarberos::Prohibido)?;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        if p.guardado(&mut cx, id).await?.estado == EstadoBarbero::Inactivo {
            return Err(ErrorBarberos::BarberoRetirado);
        }
        let bid = match p.novedades.crear_bloqueo(&mut cx, id, &bloqueo, por, ahora).await {
            Ok(bid) => bid,
            Err(ErrorPersistencia::Conflicto { restriccion }) if restriccion == RESTRICCION_BLOQUEO => {
                return Err(ErrorBarberos::BloqueoSolapado);
            }
            Err(otro) => return Err(otro.into()),
        };
        p.publicar(
            &mut cx,
            &BloqueoCreado {
                barbero: id,
                inicio: bloqueo.inicio,
                fin: bloqueo.fin,
            },
        )
        .await?;
        let dto = BloqueoDto::nuevo(
            bid,
            bloqueo.inicio,
            bloqueo.fin,
            bloqueo.motivo.codigo().to_owned(),
            bloqueo.nota.clone(),
        );
        p.auditar(&mut cx, p.entrada(actor, "crear_bloqueo", id).despues(&dto))
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    pub(crate) async fn eliminar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        bloqueo: IdBloqueo,
    ) -> Result<(), ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let eliminado = p
            .novedades
            .eliminar_bloqueo(&mut cx, id, bloqueo)
            .await?
            .ok_or(ErrorBarberos::RecursoNoEncontrado("El bloqueo"))?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "eliminar_bloqueo", id)
                .antes(&bloqueo_dto(eliminado)),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}
