//! Cableado completo sobre PostgreSQL real con **todas** las migraciones: la composición, el
//! enrutador con sus capas (origen, sesión) y los módulos conectados entre sí.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use nucleo::{Actor, RelojSistema};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::composicion::Composicion;
use crate::{enrutador, migraciones};

const SITIO: &str = "https://diamante.co";
const CLAVE: &str = "frase de prueba bien larga";

struct Respuesta {
    estado: StatusCode,
    cabeceras: axum::http::HeaderMap,
    json: Value,
}

async fn pedir(
    app: &Router,
    metodo: Method,
    ruta: &str,
    sesion: Option<&str>,
    cuerpo: Option<Value>,
) -> Respuesta {
    // Como el navegador: las peticiones que modifican datos llevan `Origin`.
    let mut peticion = Request::builder()
        .method(metodo)
        .uri(ruta)
        .header(header::ORIGIN, SITIO);
    if let Some(token) = sesion {
        peticion = peticion.header(header::COOKIE, format!("__Host-sesion={token}"));
    }
    let peticion = match cuerpo {
        Some(json) => peticion
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => peticion.body(Body::empty()).unwrap(),
    };
    let respuesta = app.clone().oneshot(peticion).await.unwrap();
    let (partes, cuerpo) = respuesta.into_parts();
    let bytes = cuerpo.collect().await.unwrap().to_bytes();
    Respuesta {
        estado: partes.status,
        cabeceras: partes.headers,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

async fn entrar(app: &Router, correo: &str) -> (Value, String) {
    let r = pedir(
        app,
        Method::POST,
        "/api/v1/auth/sesion",
        None,
        Some(json!({ "correo": correo, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let cookie = r.cabeceras[header::SET_COOKIE].to_str().unwrap();
    let token = cookie
        .strip_prefix("__Host-sesion=")
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    (r.json, token)
}

#[tokio::test]
async fn administradora_y_barbero_de_punta_a_punta() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        _ => None,
    })
    .unwrap();
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);

    // Primera administradora, como `diamante crear-administrador`.
    let invitacion = composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
        .await
        .unwrap();
    let token = invitacion.enlace.split_once("#token=").unwrap().1;
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);

    let (sesion, admin) = entrar(&app, "laura@diamante.co").await;
    assert_eq!(sesion["panel"], "/admin");

    // Los cambios del negocio quedan auditados con la administradora como autora.
    let datos = json!({ "nombre_comercial": "Diamante Barbería", "porcentaje_comision_defecto": 45 });
    let r = pedir(&app, Method::PUT, "/api/v1/negocio", Some(&admin), Some(datos)).await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/auditoria?entidad=negocio",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["total"], 1);
    assert_eq!(r.json["datos"][0]["usuario_id"], sesion["usuario"]["id"]);

    // Un barbero (sus datos los registrará el módulo `barberos` en la fase 2).
    let hash = plataforma::seguridad::cifrar_contrasena(CLAVE.into())
        .await
        .unwrap();
    sqlx::query(
        "WITH u AS (
             INSERT INTO identidad.usuario (correo, rol, hash_contrasena)
             VALUES ('santiago@diamante.co', 'barbero', $1) RETURNING id
         )
         INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos,
                                       fecha_nacimiento, celular, especialidad, fecha_ingreso, tipo_vinculacion,
                                       porcentaje_comision)
         SELECT id, 'CC', '1020304050', 'Santiago', 'Ruiz', '1995-03-02', '3105559917', 'Fade', '2025-01-15',
                'prestacion_servicios', 50
         FROM u",
    )
    .bind(hash)
    .execute(&base.pool)
    .await
    .unwrap();
    let (sesion, barbero) = entrar(&app, "santiago@diamante.co").await;
    assert_eq!(
        (sesion["panel"].as_str(), sesion["usuario"]["nombre"].as_str()),
        (Some("/barbero"), Some("Santiago Ruiz"))
    );
    let r = pedir(&app, Method::GET, "/api/v1/negocio", Some(&barbero), None).await;
    assert_eq!(r.estado, StatusCode::FORBIDDEN);
    let r = pedir(&app, Method::GET, "/api/v1/administradores", Some(&barbero), None).await;
    assert_eq!(r.estado, StatusCode::FORBIDDEN);

    // E2E-AUT-06: al suspenderlo, su siguiente petición ya no tiene sesión.
    sqlx::query(
        "INSERT INTO barberos.ausencia (barbero_id, tipo, desde, hasta, registrada_por)
         SELECT b.id, 'suspension', (now() AT TIME ZONE 'America/Bogota')::date,
                (now() AT TIME ZONE 'America/Bogota')::date + 3, u.id
         FROM barberos.barbero b, identidad.usuario u
         WHERE u.correo = 'laura@diamante.co'",
    )
    .execute(&base.pool)
    .await
    .unwrap();
    let r = pedir(&app, Method::GET, "/api/v1/auth/sesion", Some(&barbero), None).await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/auth/sesion",
        None,
        Some(json!({ "correo": "santiago@diamante.co", "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::FORBIDDEN, Some("acceso_suspendido"))
    );

    base.eliminar().await;
}
