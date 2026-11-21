use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use nucleo::identificadores::IdDiaEspecial;
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};

use super::respuestas::Creado;
use super::solicitudes::{
    ActualizarDatosSolicitud, NuevaPoliticaSolicitud, NuevoDiaSolicitud, RangoSolicitud,
};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::dto::{
    DatosNegocioDto, DiaEspecialDto, FestivosPrecargadosDto, HorarioDiaDto, PoliticaDto,
};

type Casos<C> = State<Arc<CasosDeUso<C>>>;

pub(crate) async fn obtener_datos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<DatosNegocioDto>, ErrorHttp> {
    Ok(Json(casos.obtener_datos.ejecutar(&actor).await?))
}

pub(crate) async fn actualizar_datos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<ActualizarDatosSolicitud>,
) -> Result<Json<DatosNegocioDto>, ErrorHttp> {
    Ok(Json(
        casos.actualizar_datos.ejecutar(&actor, solicitud.into()).await?,
    ))
}

pub(crate) async fn listar_dias<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(rango): Parametros<RangoSolicitud>,
) -> Result<Json<Vec<DiaEspecialDto>>, ErrorHttp> {
    Ok(Json(
        casos
            .listar_dias
            .ejecutar(&actor, rango.desde, rango.hasta)
            .await?,
    ))
}

pub(crate) async fn registrar_dia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<NuevoDiaSolicitud>,
) -> Result<Creado<DiaEspecialDto>, ErrorHttp> {
    let dia = casos.registrar_dia.ejecutar(&actor, solicitud.into()).await?;
    Ok(Creado {
        ubicacion: format!("/api/v1/negocio/dias-especiales/{}", dia.id),
        cuerpo: dia,
    })
}

pub(crate) async fn eliminar_dia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ErrorHttp> {
    let Path(id) = id.map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    casos.eliminar_dia.ejecutar(&actor, IdDiaEspecial(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn identificador(id: Result<Path<i64>, PathRejection>) -> Result<i64, ErrorHttp> {
    id.map(|Path(id)| id)
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

pub(crate) async fn editar_dia<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
    Entrada(solicitud): Entrada<NuevoDiaSolicitud>,
) -> Result<Json<DiaEspecialDto>, ErrorHttp> {
    let id = IdDiaEspecial(identificador(id)?);
    Ok(Json(
        casos.editar_dia.ejecutar(&actor, id, solicitud.into()).await?,
    ))
}

/// Idempotente: repetirla no duplica nada.
pub(crate) async fn precargar_festivos<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    anio: Result<Path<i32>, PathRejection>,
) -> Result<Json<FestivosPrecargadosDto>, ErrorHttp> {
    let Path(anio) = anio.map_err(|_| ErrorHttp::solicitud_invalida("El año debe ser un número."))?;
    Ok(Json(casos.precargar_festivos.ejecutar(&actor, anio).await?))
}

pub(crate) async fn horario_atencion<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<Vec<HorarioDiaDto>>, ErrorHttp> {
    Ok(Json(casos.horario_atencion.ejecutar(&actor).await?))
}

pub(crate) async fn listar_politicas<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<Vec<PoliticaDto>>, ErrorHttp> {
    Ok(Json(casos.listar_politicas.ejecutar(&actor).await?))
}

pub(crate) async fn publicar_politica<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<NuevaPoliticaSolicitud>,
) -> Result<Creado<PoliticaDto>, ErrorHttp> {
    let politica = casos.publicar_politica.ejecutar(&actor, solicitud.into()).await?;
    Ok(Creado {
        ubicacion: format!("/api/v1/negocio/politicas/{}", politica.id),
        cuerpo: politica,
    })
}
