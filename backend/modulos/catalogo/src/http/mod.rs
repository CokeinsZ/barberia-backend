//! Rutas HTTP del módulo (relativas a `/api/v1`, `docs/diseno/api.md`, sección 2.5).

use std::sync::Arc;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use nucleo::identificadores::IdServicio;
use plataforma::http::{ActorAutenticado, Entrada, ErrorHttp, Parametros};
use serde::Deserialize;

use crate::aplicacion::CasosDeUso;
use crate::aplicacion::comandos::NuevoServicio;
use crate::aplicacion::dto::{CategoriaDto, ServicioDto};
use crate::dominio::PropuestaServicio;

type Casos<C> = State<Arc<CasosDeUso<C>>>;

pub(crate) fn rutas<C: Send + 'static>(casos: Arc<CasosDeUso<C>>) -> Router {
    Router::new()
        .route("/categorias", get(listar_categorias::<C>))
        .route("/servicios", get(listar::<C>).post(crear::<C>))
        .route("/servicios/{id}", get(obtener::<C>).put(editar::<C>))
        .route("/servicios/{id}/estado", patch(cambiar_estado::<C>))
        .route("/servicios/{id}/componentes", put(componentes::<C>))
        .with_state(casos)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListadoParametros {
    incluir_inactivos: Option<bool>,
}

/// Cuerpo de `POST` y `PUT /servicios`. `tipo` por defecto es `simple`; en la edición, si se
/// omite, se conserva.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServicioSolicitud {
    tipo: Option<String>,
    categoria_id: i64,
    nombre: String,
    descripcion: String,
    precio: i64,
    duracion_min: Option<i64>,
    #[serde(default = "verdadero")]
    agendable_en_linea: bool,
    #[serde(default = "verdadero")]
    visible_en_landing: bool,
    #[serde(default)]
    destacado: bool,
    etiqueta: Option<String>,
    #[serde(default)]
    orden: i64,
    /// Solo al crear un combo.
    #[serde(default)]
    componentes: Vec<i64>,
}

fn verdadero() -> bool {
    true
}

impl ServicioSolicitud {
    fn propuesta(self, tipo_por_defecto: &str) -> (PropuestaServicio, Vec<IdServicio>) {
        let propuesta = PropuestaServicio {
            tipo: self.tipo.unwrap_or_else(|| tipo_por_defecto.to_owned()),
            categoria_id: self.categoria_id,
            nombre: self.nombre,
            descripcion: self.descripcion,
            precio: self.precio,
            duracion_min: self.duracion_min,
            agendable_en_linea: self.agendable_en_linea,
            visible_en_landing: self.visible_en_landing,
            destacado: self.destacado,
            etiqueta: self.etiqueta,
            orden: self.orden,
        };
        (propuesta, self.componentes.into_iter().map(IdServicio).collect())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EstadoSolicitud {
    activo: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentesSolicitud {
    servicio_ids: Vec<i64>,
}

fn id_servicio(id: Result<Path<i64>, PathRejection>) -> Result<IdServicio, ErrorHttp> {
    id.map(|Path(id)| IdServicio(id))
        .map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))
}

async fn listar_categorias<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<Vec<CategoriaDto>>, ErrorHttp> {
    Ok(Json(casos.listar_categorias.ejecutar(&actor).await?))
}

async fn listar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Parametros(p): Parametros<ListadoParametros>,
) -> Result<Json<Vec<ServicioDto>>, ErrorHttp> {
    Ok(Json(
        casos
            .listar_servicios
            .ejecutar(&actor, p.incluir_inactivos.unwrap_or(false))
            .await?,
    ))
}

async fn obtener<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
) -> Result<Json<ServicioDto>, ErrorHttp> {
    Ok(Json(
        casos.obtener_servicio.ejecutar(&actor, id_servicio(id)?).await?,
    ))
}

async fn crear<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<ServicioSolicitud>,
) -> Result<Response, ErrorHttp> {
    let (propuesta, componentes) = solicitud.propuesta("simple");
    let servicio = casos
        .crear_servicio
        .ejecutar(
            &actor,
            NuevoServicio {
                propuesta,
                componentes,
            },
        )
        .await?;
    let ubicacion = format!("/api/v1/servicios/{}", servicio.id);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, ubicacion)],
        Json(servicio),
    )
        .into_response())
}

async fn editar<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
    Entrada(solicitud): Entrada<ServicioSolicitud>,
) -> Result<Json<ServicioDto>, ErrorHttp> {
    let id = id_servicio(id)?;
    if !solicitud.componentes.is_empty() {
        return Err(ErrorHttp::solicitud_invalida(
            "Los componentes se cambian con PUT /servicios/{id}/componentes.",
        ));
    }
    let (propuesta, _) = solicitud.propuesta("");
    Ok(Json(casos.editar_servicio.ejecutar(&actor, id, propuesta).await?))
}

async fn cambiar_estado<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
    Entrada(solicitud): Entrada<EstadoSolicitud>,
) -> Result<Json<ServicioDto>, ErrorHttp> {
    Ok(Json(
        casos
            .cambiar_estado
            .ejecutar(&actor, id_servicio(id)?, solicitud.activo)
            .await?,
    ))
}

async fn componentes<C: Send + 'static>(
    State(casos): Casos<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
    Entrada(solicitud): Entrada<ComponentesSolicitud>,
) -> Result<Json<ServicioDto>, ErrorHttp> {
    let ids = solicitud.servicio_ids.into_iter().map(IdServicio).collect();
    Ok(Json(
        casos
            .reemplazar_componentes
            .ejecutar(&actor, id_servicio(id)?, ids)
            .await?,
    ))
}
