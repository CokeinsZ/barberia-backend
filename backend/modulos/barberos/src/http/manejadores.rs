use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use nucleo::identificadores::{IdAusencia, IdBarbero, IdBloqueo, IdDocumento, IdServicio};
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Formulario, Parametros};

use super::solicitudes::{
    AusenciaSolicitud, BloqueoSolicitud, BloqueosParametros, EdicionSolicitud, HorarioSolicitud,
    ListadoParametros, OrdenSolicitud, PrivadosSolicitud, RegistroSolicitud, RetiroSolicitud,
    ServiciosSolicitud, VisibilidadSolicitud,
};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::dto::{
    AusenciaDto, BarberoDto, BloqueoDto, DatosPrivadosDto, DocumentoDto, ExpedienteDto, FotoDto, HorarioDto,
    ListadoBarberosDto, ServicioOfrecidoDto,
};

type Casos<C> = State<Arc<CasosDeUso<C>>>;
type Ruta1 = Result<Path<i64>, PathRejection>;
type Ruta2 = Result<Path<(i64, i64)>, PathRejection>;

fn id(ruta: Ruta1) -> Result<IdBarbero, ErrorHttp> {
    ruta.map(|Path(id)| IdBarbero(id))
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

fn ids(ruta: Ruta2) -> Result<(IdBarbero, i64), ErrorHttp> {
    ruta.map(|Path((a, b))| (IdBarbero(a), b))
        .map_err(|_| ErrorHttp::solicitud_invalida("Los identificadores deben ser números."))
}

fn creado<T: serde::Serialize>(ubicacion: String, cuerpo: T) -> Response {
    (StatusCode::CREATED, [(header::LOCATION, ubicacion)], Json(cuerpo)).into_response()
}

pub(crate) async fn yo<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<BarberoDto>, ErrorHttp> {
    Ok(Json(casos.ficha.yo(&actor).await?))
}

pub(crate) async fn listar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ListadoParametros>,
) -> Result<Json<ListadoBarberosDto>, ErrorHttp> {
    Ok(Json(casos.listar.ejecutar(&actor, p.q, p.estado).await?))
}

pub(crate) async fn registrar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<RegistroSolicitud>,
) -> Result<Response, ErrorHttp> {
    let ficha = casos.registrar.ejecutar(&actor, solicitud.into()).await?;
    Ok(creado(format!("/api/v1/barberos/{}", ficha.id), ficha))
}

pub(crate) async fn ordenar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<OrdenSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    casos.ordenar.ejecutar(&actor, solicitud.ids()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn ficha<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<BarberoDto>, ErrorHttp> {
    Ok(Json(casos.ficha.ejecutar(&actor, id(ruta)?).await?))
}

pub(crate) async fn editar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<EdicionSolicitud>,
) -> Result<Json<BarberoDto>, ErrorHttp> {
    Ok(Json(
        casos.editar.ejecutar(&actor, id(ruta)?, solicitud.into()).await?,
    ))
}

pub(crate) async fn visibilidad<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<VisibilidadSolicitud>,
) -> Result<Json<BarberoDto>, ErrorHttp> {
    Ok(Json(
        casos
            .visibilidad
            .ejecutar(&actor, id(ruta)?, solicitud.into())
            .await?,
    ))
}

pub(crate) async fn retirar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<RetiroSolicitud>,
) -> Result<Json<BarberoDto>, ErrorHttp> {
    Ok(Json(
        casos
            .retirar
            .ejecutar(&actor, id(ruta)?, solicitud.fecha, &solicitud.motivo)
            .await?,
    ))
}

pub(crate) async fn horario<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<HorarioDto>, ErrorHttp> {
    Ok(Json(casos.horario.consultar(&actor, id(ruta)?).await?))
}

pub(crate) async fn cambiar_horario<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<HorarioSolicitud>,
) -> Result<Json<HorarioDto>, ErrorHttp> {
    let franjas = solicitud.franjas.into_iter().map(Into::into).collect();
    Ok(Json(casos.horario.cambiar(&actor, id(ruta)?, franjas).await?))
}

pub(crate) async fn servicios<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<Vec<ServicioOfrecidoDto>>, ErrorHttp> {
    Ok(Json(casos.servicios.consultar(&actor, id(ruta)?).await?))
}

pub(crate) async fn cambiar_servicios<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<ServiciosSolicitud>,
) -> Result<Json<Vec<ServicioOfrecidoDto>>, ErrorHttp> {
    let ids = solicitud.servicio_ids.into_iter().map(IdServicio).collect();
    Ok(Json(casos.servicios.cambiar(&actor, id(ruta)?, ids).await?))
}

pub(crate) async fn privados<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<DatosPrivadosDto>, ErrorHttp> {
    Ok(Json(casos.privados.obtener(&actor, id(ruta)?).await?))
}

pub(crate) async fn guardar_privados<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(solicitud): Entrada<PrivadosSolicitud>,
) -> Result<Json<DatosPrivadosDto>, ErrorHttp> {
    Ok(Json(
        casos
            .privados
            .guardar(&actor, id(ruta)?, solicitud.into())
            .await?,
    ))
}

/// Multipart con el campo `foto`.
pub(crate) async fn foto<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    mut formulario: Formulario,
) -> Result<Json<FotoDto>, ErrorHttp> {
    Ok(Json(
        casos
            .foto
            .ejecutar(&actor, id(ruta)?, formulario.archivo("foto"))
            .await?,
    ))
}

pub(crate) async fn documentos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<ExpedienteDto>, ErrorHttp> {
    Ok(Json(casos.documentos.listar(&actor, id(ruta)?).await?))
}

/// Multipart con `tipo`, `periodo` (solo la planilla PILA, `AAAA-MM`) y `archivo`.
pub(crate) async fn cargar_documento<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    mut formulario: Formulario,
) -> Result<Response, ErrorHttp> {
    let barbero = id(ruta)?;
    let archivo = formulario.archivo("archivo");
    let documento: DocumentoDto = casos
        .documentos
        .cargar(
            &actor,
            barbero,
            formulario.campo("tipo"),
            formulario.campo("periodo"),
            archivo,
        )
        .await?;
    Ok(creado(
        format!("/api/v1/barberos/{barbero}/documentos/{}", documento.id),
        documento,
    ))
}

pub(crate) async fn quitar_documento<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta2,
) -> Result<StatusCode, ErrorHttp> {
    let (barbero, documento) = ids(ruta)?;
    casos
        .documentos
        .quitar(&actor, barbero, IdDocumento(documento))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Redirige a una URL firmada de 5 minutos (RNF-SEG-09).
pub(crate) async fn descargar_documento<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta2,
) -> Result<Redirect, ErrorHttp> {
    let (barbero, documento) = ids(ruta)?;
    let url = casos
        .documentos
        .url_descarga(&actor, barbero, IdDocumento(documento))
        .await?;
    Ok(Redirect::to(&url))
}

pub(crate) async fn ausencias<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
) -> Result<Json<Vec<AusenciaDto>>, ErrorHttp> {
    Ok(Json(casos.ausencias.listar(&actor, id(ruta)?).await?))
}

pub(crate) async fn registrar_ausencia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(s): Entrada<AusenciaSolicitud>,
) -> Result<Response, ErrorHttp> {
    let barbero = id(ruta)?;
    let ausencia = casos
        .ausencias
        .registrar(&actor, barbero, &s.tipo, s.desde, s.hasta, &s.observacion)
        .await?;
    Ok(creado(
        format!("/api/v1/barberos/{barbero}/ausencias/{}", ausencia.id),
        ausencia,
    ))
}

pub(crate) async fn anular_ausencia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta2,
) -> Result<Json<AusenciaDto>, ErrorHttp> {
    let (barbero, ausencia) = ids(ruta)?;
    Ok(Json(
        casos
            .ausencias
            .anular(&actor, barbero, IdAusencia(ausencia))
            .await?,
    ))
}

/// Multipart con el campo `soporte`.
pub(crate) async fn soporte_ausencia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta2,
    mut formulario: Formulario,
) -> Result<Json<AusenciaDto>, ErrorHttp> {
    let (barbero, ausencia) = ids(ruta)?;
    let archivo = formulario.archivo("soporte");
    Ok(Json(
        casos
            .ausencias
            .adjuntar_soporte(&actor, barbero, IdAusencia(ausencia), archivo)
            .await?,
    ))
}

pub(crate) async fn bloqueos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Parametros(p): Parametros<BloqueosParametros>,
) -> Result<Json<Vec<BloqueoDto>>, ErrorHttp> {
    Ok(Json(casos.bloqueos.listar(&actor, id(ruta)?, p.desde).await?))
}

pub(crate) async fn crear_bloqueo<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta1,
    Entrada(s): Entrada<BloqueoSolicitud>,
) -> Result<Response, ErrorHttp> {
    let barbero = id(ruta)?;
    let bloqueo = casos.bloqueos.crear(&actor, barbero, s.into()).await?;
    Ok(creado(
        format!("/api/v1/barberos/{barbero}/bloqueos/{}", bloqueo.id),
        bloqueo,
    ))
}

pub(crate) async fn eliminar_bloqueo<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    ruta: Ruta2,
) -> Result<StatusCode, ErrorHttp> {
    let (barbero, bloqueo) = ids(ruta)?;
    casos
        .bloqueos
        .eliminar(&actor, barbero, IdBloqueo(bloqueo))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
