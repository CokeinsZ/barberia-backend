//! Catálogo de punta a punta: HTTP → caso de uso → PostgreSQL real (componentes de combos,
//! nombre único sin distinguir mayúsculas ni tildes y auditoría).
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::ModuloAuditoria;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::{Extension, Router};
use catalogo::api::OferentesServicio;
use catalogo::{DependenciasCatalogo, ModuloCatalogo};
use http_body_util::BodyExt;
use nucleo::identificadores::{IdBarbero, IdServicio, IdUsuario};
use nucleo::{Actor, ErrorPersistencia, RelojSistema};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::http::{ErrorHttp, Sesiones, ValidadorSesion};
use serde_json::{Value, json};
use tower::ServiceExt;

static MIGRACIONES_IDENTIDAD: sqlx::migrate::Migrator = sqlx::migrate!("../identidad/migraciones");
static MIGRACIONES_AUDITORIA: sqlx::migrate::Migrator = sqlx::migrate!("../auditoria/migraciones");

#[derive(Debug)]
struct SesionesDePrueba;

#[async_trait]
impl ValidadorSesion for SesionesDePrueba {
    async fn validar(&self, token: &str) -> Result<Option<Actor>, ErrorHttp> {
        Ok(match token {
            "admin" => Some(Actor::Administrador {
                usuario: IdUsuario(1),
            }),
            "barbero" => Some(Actor::Barbero {
                usuario: IdUsuario(2),
                barbero: IdBarbero(1),
            }),
            _ => None,
        })
    }
}

/// Sin barberos todavía: ningún servicio tiene quién lo ofrezca.
#[derive(Debug)]
struct SinOferentes;

#[async_trait]
impl OferentesServicio<UnidadDeTrabajo> for SinOferentes {
    async fn contar(&self, _: &mut UnidadDeTrabajo) -> Result<HashMap<IdServicio, i64>, ErrorPersistencia> {
        Ok(HashMap::new())
    }
}

async fn pedir(
    app: &Router,
    metodo: Method,
    ruta: &str,
    sesion: &str,
    cuerpo: Option<Value>,
) -> (StatusCode, Value) {
    let peticion = Request::builder()
        .method(metodo)
        .uri(ruta)
        .header(header::COOKIE, format!("__Host-sesion={sesion}"));
    let peticion = match cuerpo {
        Some(json) => peticion
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => peticion.body(Body::empty()),
    }
    .unwrap();
    let respuesta = app.clone().oneshot(peticion).await.unwrap();
    let estado = respuesta.status();
    let bytes = respuesta.into_body().collect().await.unwrap().to_bytes();
    (estado, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn servicio(nombre: &str, precio: i64) -> Value {
    json!({ "categoria_id": 1, "nombre": nombre, "descripcion": "Servicio de prueba.", "precio": precio, "duracion_min": 30 })
}

#[tokio::test]
async fn servicios_combos_y_estado() {
    let base = BaseDePrueba::crear(&[
        OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        },
        OrigenMigraciones {
            modulo: "identidad",
            migraciones: &MIGRACIONES_IDENTIDAD,
        },
        OrigenMigraciones {
            modulo: "catalogo",
            migraciones: &catalogo::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "auditoria",
            migraciones: &MIGRACIONES_AUDITORIA,
        },
    ])
    .await;
    sqlx::query("INSERT INTO identidad.usuario (correo, rol) VALUES ('laura@diamante.co', 'administrador')")
        .execute(&base.pool)
        .await
        .unwrap();
    let auditor = ModuloAuditoria::con_postgres(base.pool.clone(), Arc::new(RelojSistema)).auditor();
    let modulo = ModuloCatalogo::con_postgres(DependenciasCatalogo {
        pool: base.pool.clone(),
        auditor,
        oferentes: Arc::new(SinOferentes),
    });
    let app = Router::new()
        .nest("/api/v1", modulo.rutas())
        .layer(Extension(Sesiones(Arc::new(SesionesDePrueba))));

    let (estado, categorias) = pedir(&app, Method::GET, "/api/v1/categorias", "barbero", None).await;
    assert_eq!(
        (estado, categorias.as_array().unwrap().len()),
        (StatusCode::OK, 4)
    );

    let (estado, corte) = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        "admin",
        Some(servicio("Corte clásico", 35_000)),
    )
    .await;
    assert_eq!(estado, StatusCode::CREATED, "{corte}");
    let (_, cejas) = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        "admin",
        Some(servicio("Diseño de cejas", 10_000)),
    )
    .await;
    assert_eq!(
        corte["advertencia"],
        "Ningún barbero ofrece este servicio: no aparece en el agendamiento."
    );

    // El nombre es único sin importar mayúsculas ni tildes.
    let (estado, error) = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        "admin",
        Some(servicio("CORTE CLASICO", 1)),
    )
    .await;
    assert_eq!(
        (estado, error["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("nombre_en_uso"))
    );

    let combo = json!({
        "tipo": "combo", "categoria_id": 3, "nombre": "Corte + cejas", "descripcion": "Combo.", "precio": 40000,
        "duracion_min": 60, "destacado": true, "etiqueta": "Ahorras $5.000", "componentes": [corte["id"], cejas["id"]]
    });
    let (estado, combo) = pedir(&app, Method::POST, "/api/v1/servicios", "admin", Some(combo)).await;
    assert_eq!(estado, StatusCode::CREATED, "{combo}");
    assert_eq!(combo["componentes"].as_array().unwrap().len(), 2);
    assert_eq!(
        (combo["categoria"]["nombre"].as_str(), combo["etiqueta"].as_str()),
        (Some("Combos"), Some("Ahorras $5.000"))
    );

    // Un combo con un solo componente no se acepta.
    let ruta = format!("/api/v1/servicios/{}/componentes", combo["id"]);
    let (estado, _) = pedir(
        &app,
        Method::PUT,
        &ruta,
        "admin",
        Some(json!({ "servicio_ids": [corte["id"]] })),
    )
    .await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);

    // Nuevo precio y desactivar: el inactivo desaparece del listado por defecto.
    let ruta = format!("/api/v1/servicios/{}", corte["id"]);
    let (estado, editado) = pedir(
        &app,
        Method::PUT,
        &ruta,
        "admin",
        Some(servicio("Corte clásico", 38_000)),
    )
    .await;
    assert_eq!(
        (estado, editado["precio"].as_i64()),
        (StatusCode::OK, Some(38_000))
    );
    let ruta = format!("/api/v1/servicios/{}/estado", cejas["id"]);
    let (estado, _) = pedir(
        &app,
        Method::PATCH,
        &ruta,
        "admin",
        Some(json!({ "activo": false })),
    )
    .await;
    assert_eq!(estado, StatusCode::OK);
    let (_, activos) = pedir(&app, Method::GET, "/api/v1/servicios", "barbero", None).await;
    assert_eq!(activos.as_array().unwrap().len(), 2);
    let (_, todos) = pedir(
        &app,
        Method::GET,
        "/api/v1/servicios?incluir_inactivos=true",
        "admin",
        None,
    )
    .await;
    assert_eq!(todos.as_array().unwrap().len(), 3);
    let (estado, _) = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        "barbero",
        Some(servicio("Barba", 20_000)),
    )
    .await;
    assert_eq!(estado, StatusCode::FORBIDDEN);

    let acciones: Vec<String> = sqlx::query_scalar("SELECT accion FROM auditoria.registro ORDER BY id")
        .fetch_all(&base.pool)
        .await
        .unwrap();
    assert_eq!(
        acciones,
        [
            "crear_servicio",
            "crear_servicio",
            "crear_servicio",
            "editar_servicio",
            "desactivar_servicio"
        ]
    );
    base.eliminar().await;
}
