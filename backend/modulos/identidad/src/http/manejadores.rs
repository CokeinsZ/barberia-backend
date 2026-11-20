use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use nucleo::identificadores::IdUsuario;
use plataforma::http::{
    ActorAutenticado, Entrada, ErrorHttp, IpCliente, TokenSesion, cookie_sesion, cookie_sesion_vencida,
};
use plataforma::seguridad::huella_token;
use serde_json::json;

use super::rutas::EstadoHttp;
use super::solicitudes::{
    CambioContrasenaSolicitud, EnlaceSolicitud, EstadoAdministradorSolicitud, InicioSesionSolicitud,
    NuevoAdministradorSolicitud, RecuperacionSolicitud,
};
use crate::aplicacion::comandos::SolicitudInicioSesion;
use crate::aplicacion::dto::{AdministradorDto, SesionDto};

type Estado<C> = State<EstadoHttp<C>>;

pub(crate) async fn iniciar_sesion<C: Send + 'static>(
    State(estado): Estado<C>,
    IpCliente(ip): IpCliente,
    encabezados: HeaderMap,
    Entrada(solicitud): Entrada<InicioSesionSolicitud>,
) -> Result<Response, ErrorHttp> {
    estado.limites.inicio_sesion(ip)?;
    let agente = encabezados
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let iniciada = estado
        .casos
        .iniciar_sesion
        .ejecutar(SolicitudInicioSesion {
            correo: solicitud.correo,
            contrasena: solicitud.contrasena,
            recordarme: solicitud.recordarme,
            ip,
            agente,
        })
        .await?;
    let cookie = cookie_sesion(&iniciada.token, iniciada.duracion.duracion_cookie())?;
    Ok(([(header::SET_COOKIE, cookie)], Json(iniciada.sesion)).into_response())
}

pub(crate) async fn consultar_sesion<C: Send + 'static>(
    State(estado): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<SesionDto>, ErrorHttp> {
    Ok(Json(estado.casos.consultar_sesion.ejecutar(&actor).await?))
}

/// Siempre responde 204 y borra la cookie, haya o no sesión.
pub(crate) async fn cerrar_sesion<C: Send + 'static>(
    State(estado): Estado<C>,
    TokenSesion(token): TokenSesion,
) -> Result<Response, ErrorHttp> {
    estado.casos.cerrar_sesion.ejecutar(token.as_deref()).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie_sesion_vencida())],
    )
        .into_response())
}

pub(crate) async fn activar_cuenta<C: Send + 'static>(
    State(estado): Estado<C>,
    Entrada(solicitud): Entrada<EnlaceSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    estado
        .casos
        .activar_cuenta
        .ejecutar(&solicitud.token, solicitud.contrasena)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Responde 202 exista o no la cuenta.
pub(crate) async fn solicitar_recuperacion<C: Send + 'static>(
    State(estado): Estado<C>,
    IpCliente(ip): IpCliente,
    Entrada(solicitud): Entrada<RecuperacionSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    estado.limites.recuperacion(ip, &solicitud.correo)?;
    estado
        .casos
        .solicitar_recuperacion
        .ejecutar(&solicitud.correo)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub(crate) async fn restablecer_contrasena<C: Send + 'static>(
    State(estado): Estado<C>,
    Entrada(solicitud): Entrada<EnlaceSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    estado
        .casos
        .restablecer_contrasena
        .ejecutar(&solicitud.token, solicitud.contrasena)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Cierra las demás sesiones; la actual sigue abierta.
pub(crate) async fn cambiar_contrasena<C: Send + 'static>(
    State(estado): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    TokenSesion(token): TokenSesion,
    Entrada(solicitud): Entrada<CambioContrasenaSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    let actual = token.as_deref().map(huella_token);
    estado
        .casos
        .cambiar_contrasena
        .ejecutar(&actor, actual.as_ref(), &solicitud.actual, solicitud.nueva)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn listar_administradores<C: Send + 'static>(
    State(estado): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
) -> Result<Json<Vec<AdministradorDto>>, ErrorHttp> {
    Ok(Json(estado.casos.listar_administradores.ejecutar(&actor).await?))
}

/// 201 con el identificador. El enlace de la invitación viaja solo por correo.
pub(crate) async fn invitar_administrador<C: Send + 'static>(
    State(estado): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    Entrada(solicitud): Entrada<NuevoAdministradorSolicitud>,
) -> Result<Response, ErrorHttp> {
    let invitacion = estado
        .casos
        .invitar_administrador
        .ejecutar(&actor, &solicitud.correo, &solicitud.nombre)
        .await?;
    let id = invitacion.usuario;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/api/v1/administradores/{id}"))],
        Json(json!({ "id": id })),
    )
        .into_response())
}

pub(crate) async fn cambiar_estado_administrador<C: Send + 'static>(
    State(estado): Estado<C>,
    ActorAutenticado(actor): ActorAutenticado,
    id: Result<Path<i64>, PathRejection>,
    Entrada(solicitud): Entrada<EstadoAdministradorSolicitud>,
) -> Result<StatusCode, ErrorHttp> {
    let Path(id) = id.map_err(|_| ErrorHttp::solicitud_invalida("El identificador debe ser un número."))?;
    estado
        .casos
        .cambiar_estado_administrador
        .ejecutar(&actor, IdUsuario(id), solicitud.activo)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
