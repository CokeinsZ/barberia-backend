use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use nucleo::identificadores::IdDiaEspecial;
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};

use super::respuestas::Creado;
use super::solicitudes::{ActualizarDatosSolicitud, NuevoDiaSolicitud, RangoSolicitud};
use crate::aplicacion::CasosDeUso;
use crate::aplicacion::dto::{DatosNegocioDto, DiaEspecialDto};

type Casos = State<Arc<CasosDeUso>>;

pub(crate) async fn obtener_datos(
    State(casos): Casos,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<DatosNegocioDto>, ErrorHttp> {
    Ok(Json(casos.obtener_datos.ejecutar(&actor).await?))
}

pub(crate) async fn actualizar_datos(
    State(casos): Casos,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<ActualizarDatosSolicitud>,
) -> Result<Json<DatosNegocioDto>, ErrorHttp> {
    Ok(Json(
        casos.actualizar_datos.ejecutar(&actor, solicitud.into()).await?,
    ))
}

pub(crate) async fn listar_dias(
    State(casos): Casos,
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

pub(crate) async fn registrar_dia(
    State(casos): Casos,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<NuevoDiaSolicitud>,
) -> Result<Creado<DiaEspecialDto>, ErrorHttp> {
    let dia = casos.registrar_dia.ejecutar(&actor, solicitud.into()).await?;
    Ok(Creado {
        ubicacion: format!("/api/v1/negocio/dias-especiales/{}", dia.id),
        cuerpo: dia,
    })
}

pub(crate) async fn eliminar_dia(
    State(casos): Casos,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ErrorHttp> {
    let Path(id) = id.map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    casos.eliminar_dia.ejecutar(&actor, IdDiaEspecial(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
