//! Prueba de integración de punta a punta del módulo: HTTP → caso de uso → repositorio →
//! PostgreSQL real, sobre una base efímera con las migraciones reales.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

// En un archivo de pruebas, un `unwrap` que falla es exactamente lo que se quiere: la prueba falla.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use async_trait::async_trait;
use auditoria::ModuloAuditoria;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use negocio::ModuloNegocio;
use nucleo::identificadores::{IdBarbero, IdUsuario};
use nucleo::{Actor, RelojControlado};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones};
use plataforma::http::{ErrorHttp, Sesiones, ValidadorSesion};
use serde_json::{Value, json};
use tower::ServiceExt;

/// `auditoria.registro` referencia a `identidad.usuario`.
static MIGRACIONES_IDENTIDAD: sqlx::migrate::Migrator = sqlx::migrate!("../identidad/migraciones");
// El horario de atención sale de la vista publicada de `barberos` (que referencia a `archivos` y `catalogo`).
static MIGRACIONES_ARCHIVOS: sqlx::migrate::Migrator = sqlx::migrate!("../archivos/migraciones");
static MIGRACIONES_CATALOGO: sqlx::migrate::Migrator = sqlx::migrate!("../catalogo/migraciones");
static MIGRACIONES_BARBEROS: sqlx::migrate::Migrator = sqlx::migrate!("../barberos/migraciones");

/// Sesiones de prueba: el token dice quién es.
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

struct Escenario {
    base: BaseDePrueba,
    enrutador: Router,
}

impl Escenario {
    /// Miércoles 18 de noviembre de 2026, 2:50 p. m. en Bogotá.
    async fn nuevo() -> Self {
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
                modulo: "negocio",
                migraciones: &negocio::MIGRACIONES,
            },
            OrigenMigraciones {
                modulo: "auditoria",
                migraciones: &auditoria::MIGRACIONES,
            },
            OrigenMigraciones {
                modulo: "archivos",
                migraciones: &MIGRACIONES_ARCHIVOS,
            },
            OrigenMigraciones {
                modulo: "catalogo",
                migraciones: &MIGRACIONES_CATALOGO,
            },
            OrigenMigraciones {
                modulo: "barberos",
                migraciones: &MIGRACIONES_BARBEROS,
            },
        ])
        .await;
        // Las cuentas de las sesiones de prueba, para la llave foránea de la auditoría.
        sqlx::query(
            "INSERT INTO identidad.usuario (correo, rol) VALUES ('laura@diamante.co', 'administrador'),
                                                                ('santiago@diamante.co', 'barbero')",
        )
        .execute(&base.pool)
        .await
        .unwrap();
        let reloj = Arc::new(RelojControlado::en("2026-11-18T19:50:00Z".parse().unwrap()));
        let auditor = ModuloAuditoria::con_postgres(base.pool.clone(), reloj.clone()).auditor();
        let modulo = ModuloNegocio::con_postgres(base.pool.clone(), reloj, auditor);
        let enrutador = Router::new()
            .nest("/api/v1", modulo.rutas())
            .layer(Extension(Sesiones(Arc::new(SesionesDePrueba))));
        Self { base, enrutador }
    }

    async fn pedir(
        &self,
        metodo: Method,
        ruta: &str,
        sesion: Option<&str>,
        cuerpo: Option<Value>,
    ) -> Respuesta {
        let mut peticion = Request::builder().method(metodo).uri(ruta);
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
        let respuesta = self.enrutador.clone().oneshot(peticion).await.unwrap();
        let (partes, cuerpo) = respuesta.into_parts();
        let bytes = cuerpo.collect().await.unwrap().to_bytes();
        Respuesta {
            estado: partes.status,
            cabeceras: partes.headers,
            json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }
}

struct Respuesta {
    estado: StatusCode,
    cabeceras: axum::http::HeaderMap,
    json: Value,
}

#[tokio::test]
async fn sin_sesion_responde_401_en_formato_problem_details() {
    let e = Escenario::nuevo().await;
    let r = e.pedir(Method::GET, "/api/v1/negocio", None, None).await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);
    assert_eq!(r.cabeceras[header::CONTENT_TYPE], "application/problem+json");
    assert_eq!(r.json["codigo"], "no_autenticado");
    let r = e
        .pedir(Method::GET, "/api/v1/negocio", Some("token-falso"), None)
        .await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);
    e.base.eliminar().await;
}

#[tokio::test]
async fn el_administrador_lee_y_actualiza_los_datos_del_negocio() {
    let e = Escenario::nuevo().await;

    let r = e.pedir(Method::GET, "/api/v1/negocio", Some("admin"), None).await;
    assert_eq!(r.estado, StatusCode::OK);
    assert_eq!(
        r.json["nombre_comercial"], "Diamante Barbería",
        "dato inicial de la migración"
    );
    assert_eq!(r.json["porcentaje_comision_defecto"], 50.0);

    let cambios = json!({
        "nombre_comercial": "Diamante Barbería",
        "nit": "900.123.456-7",
        "whatsapp": "+57 300 555 0000",
        "instagram": "@diamante.barberia",
        "porcentaje_comision_defecto": 45.5
    });
    let r = e
        .pedir(Method::PUT, "/api/v1/negocio", Some("admin"), Some(cambios))
        .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);

    let r = e.pedir(Method::GET, "/api/v1/negocio", Some("admin"), None).await;
    assert_eq!(r.json["whatsapp"], "3005550000");
    assert_eq!(r.json["instagram"], "diamante.barberia");
    assert_eq!(r.json["porcentaje_comision_defecto"], 45.5);

    // El cambio queda auditado en la misma transacción, con el antes y el después.
    let (usuario, actor, antes, despues): (Option<i64>, String, Value, Value) = sqlx::query_as(
        "SELECT usuario_id, actor::text, antes, despues FROM auditoria.registro WHERE accion = 'actualizar_datos'",
    )
    .fetch_one(&e.base.pool)
    .await
    .unwrap();
    assert_eq!((usuario, actor.as_str()), (Some(1), "administrador"));
    assert_eq!(antes["porcentaje_comision_defecto"], 50.0);
    assert_eq!(despues["porcentaje_comision_defecto"], 45.5);
    e.base.eliminar().await;
}

#[tokio::test]
async fn el_barbero_no_puede_cambiar_los_datos_del_negocio() {
    let e = Escenario::nuevo().await;
    let cambios = json!({ "nombre_comercial": "Otro nombre", "porcentaje_comision_defecto": 90 });
    let r = e
        .pedir(Method::PUT, "/api/v1/negocio", Some("barbero"), Some(cambios))
        .await;
    assert_eq!(r.estado, StatusCode::FORBIDDEN);
    assert_eq!(r.json["codigo"], "prohibido");
    e.base.eliminar().await;
}

#[tokio::test]
async fn los_datos_invalidos_responden_422_con_los_campos() {
    let e = Escenario::nuevo().await;
    let cambios =
        json!({ "nombre_comercial": "D", "whatsapp": "210 555 9917", "porcentaje_comision_defecto": 150 });
    let r = e
        .pedir(Method::PUT, "/api/v1/negocio", Some("admin"), Some(cambios))
        .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY);
    let campos: Vec<_> = r.json["errores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["campo"].clone())
        .collect();
    assert_eq!(
        campos,
        ["nombre_comercial", "whatsapp", "porcentaje_comision_defecto"]
    );

    let r = e
        .pedir(
            Method::PUT,
            "/api/v1/negocio",
            Some("admin"),
            Some(json!({ "campo_raro": 1 })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["codigo"], "solicitud_invalida");
    e.base.eliminar().await;
}

#[tokio::test]
async fn ciclo_completo_de_un_dia_especial() {
    let e = Escenario::nuevo().await;
    let nochebuena = json!({
        "fecha": "2026-12-24", "nombre": "Nochebuena", "tipo": "cierre_temprano", "hora_cierre": "14:00:00"
    });

    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/dias-especiales",
            Some("admin"),
            Some(nochebuena.clone()),
        )
        .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let id = r.json["id"].as_i64().unwrap();
    assert_eq!(
        r.cabeceras[header::LOCATION],
        format!("/api/v1/negocio/dias-especiales/{id}")
    );
    assert_eq!(r.json["tipo"], "cierre_temprano");
    assert_eq!(r.json["hora_cierre"], "14:00:00");

    // La restricción única de la base de datos llega como un 409 con mensaje claro.
    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/dias-especiales",
            Some("admin"),
            Some(nochebuena),
        )
        .await;
    assert_eq!(r.estado, StatusCode::CONFLICT);
    assert_eq!(r.json["codigo"], "fecha_ocupada");
    assert_eq!(
        r.json["title"],
        "Ya hay un día especial registrado el 24/12/2026."
    );

    // El barbero puede consultar el calendario (lo necesita para su agenda).
    let r = e
        .pedir(
            Method::GET,
            "/api/v1/negocio/dias-especiales?desde=2026-12-01&hasta=2026-12-31",
            Some("barbero"),
            None,
        )
        .await;
    assert_eq!(r.estado, StatusCode::OK);
    assert_eq!(r.json.as_array().unwrap().len(), 1);
    assert_eq!(r.json[0]["fecha"], "2026-12-24");

    let ruta = format!("/api/v1/negocio/dias-especiales/{id}");
    assert_eq!(
        e.pedir(Method::DELETE, &ruta, Some("admin"), None).await.estado,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.pedir(Method::DELETE, &ruta, Some("admin"), None).await.estado,
        StatusCode::NOT_FOUND
    );
    let acciones: Vec<String> = sqlx::query_scalar("SELECT accion FROM auditoria.registro ORDER BY id")
        .fetch_all(&e.base.pool)
        .await
        .unwrap();
    assert_eq!(acciones, ["registrar_dia_especial", "eliminar_dia_especial"]);
    e.base.eliminar().await;
}

#[tokio::test]
async fn no_se_registran_dias_en_el_pasado() {
    let e = Escenario::nuevo().await;
    let ayer = json!({ "fecha": "2026-11-17", "nombre": "Ayer", "tipo": "cerrado" });
    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/dias-especiales",
            Some("admin"),
            Some(ayer),
        )
        .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.json["errores"][0]["campo"], "fecha");
    e.base.eliminar().await;
}

#[tokio::test]
async fn festivos_politicas_y_horario_de_atencion() {
    let e = Escenario::nuevo().await;

    // Festivos de 2027 (el reloj está en noviembre de 2026): los 18, y repetir no duplica.
    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/dias-especiales/festivos/2027",
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["creados"].as_array().unwrap().len(), 18);
    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/dias-especiales/festivos/2027",
            Some("admin"),
            None,
        )
        .await;
    assert_eq!(
        (
            r.json["creados"].as_array().unwrap().len(),
            r.json["omitidos"].as_u64()
        ),
        (0, Some(18))
    );

    // Editar uno precargado lo vuelve manual.
    let r = e
        .pedir(
            Method::GET,
            "/api/v1/negocio/dias-especiales?desde=2027-01-01&hasta=2027-01-31",
            Some("barbero"),
            None,
        )
        .await;
    let reyes = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["fecha"] == "2027-01-11")
        .unwrap()
        .clone();
    assert_eq!(reyes["automatico"], true);
    let cambios = json!({ "fecha": "2027-01-11", "nombre": "Reyes (cerramos temprano)", "tipo": "cierre_temprano", "hora_cierre": "14:00:00" });
    let r = e
        .pedir(
            Method::PUT,
            &format!("/api/v1/negocio/dias-especiales/{}", reyes["id"]),
            Some("admin"),
            Some(cambios),
        )
        .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(
        (r.json["tipo"].as_str(), r.json["automatico"].as_bool()),
        (Some("cierre_temprano"), Some(false))
    );

    // Políticas: publicar, repetir la versión (422) y listar con la vigente.
    let politica = json!({ "tipo": "tratamiento_datos", "version": "2026.1", "url": "/legal/tratamiento-de-datos", "vigente_desde": "2026-01-01T00:00:00Z" });
    let r = e
        .pedir(
            Method::POST,
            "/api/v1/negocio/politicas",
            Some("admin"),
            Some(politica.clone()),
        )
        .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    assert_eq!(
        e.pedir(
            Method::POST,
            "/api/v1/negocio/politicas",
            Some("admin"),
            Some(politica)
        )
        .await
        .estado,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let r = e
        .pedir(Method::GET, "/api/v1/negocio/politicas", Some("admin"), None)
        .await;
    assert_eq!(
        (r.json[0]["version"].as_str(), r.json[0]["vigente"].as_bool()),
        (Some("2026.1"), Some(true))
    );
    assert_eq!(
        e.pedir(Method::GET, "/api/v1/negocio/politicas", Some("barbero"), None)
            .await
            .estado,
        StatusCode::FORBIDDEN
    );

    // Horario de atención: de la franja más temprana a la más tardía de los barberos vigentes.
    sqlx::raw_sql(
        "INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos, fecha_nacimiento,
                                       celular, especialidad, fecha_ingreso, tipo_vinculacion, porcentaje_comision)
         VALUES (2, 'CC', '1020304050', 'Santiago', 'Ruiz', '1995-03-02', '3105559917', 'Fade', '2025-01-15', 'prestacion_servicios', 50);
         INSERT INTO identidad.usuario (correo, rol) VALUES ('mateo@diamante.co', 'barbero');
         INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos, fecha_nacimiento,
                                       celular, especialidad, fecha_ingreso, tipo_vinculacion, porcentaje_comision, fecha_retiro, motivo_retiro)
         VALUES (3, 'CC', '1020304051', 'Mateo', 'Gil', '1990-03-02', '3105559918', 'Barba', '2024-01-15', 'prestacion_servicios', 50,
                 '2026-01-31', 'renuncia_voluntaria');
         INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin) VALUES
           (1, 1, '09:00', '13:00'), (1, 1, '14:00', '19:00'), (1, 6, '08:00', '16:00'),
           (2, 1, '06:00', '22:00');",
    )
    .execute(&e.base.pool)
    .await
    .unwrap();
    let r = e
        .pedir(
            Method::GET,
            "/api/v1/negocio/horario-atencion",
            Some("barbero"),
            None,
        )
        .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(
        (r.json[0]["abre"].as_str(), r.json[0]["cierra"].as_str()),
        (Some("09:00:00"), Some("19:00:00")),
        "el retirado no cuenta"
    );
    assert_eq!(r.json[5]["abre"], "08:00:00");
    assert_eq!(
        (r.json[6]["nombre"].as_str(), r.json[6]["abierto"].as_bool()),
        (Some("Domingo"), Some(false))
    );
    e.base.eliminar().await;
}
