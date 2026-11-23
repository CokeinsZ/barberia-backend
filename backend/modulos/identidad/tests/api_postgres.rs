//! Flujo completo del personal sobre PostgreSQL real y Argon2 real: invitación, activación,
//! inicio de sesión, recuperación, administradores, auditoría y correos encolados.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use async_trait::async_trait;
use auditoria::ModuloAuditoria;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use identidad::api::{PerfilBarbero, PerfilesBarberos};
use identidad::{DependenciasIdentidad, ModuloIdentidad};
use notificaciones::ModuloNotificaciones;
use nucleo::identificadores::IdUsuario;
use nucleo::{Actor, ErrorPersistencia, RelojSistema};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::http::Sesiones;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Sin barberos: este módulo solo prueba cuentas de administradores.
#[derive(Debug)]
struct SinBarberos;

#[async_trait]
impl PerfilesBarberos<UnidadDeTrabajo> for SinBarberos {
    async fn por_usuario(
        &self,
        _: &mut UnidadDeTrabajo,
        _: IdUsuario,
    ) -> Result<Option<PerfilBarbero>, ErrorPersistencia> {
        Ok(None)
    }
}

const CLAVE: &str = "frase de prueba bien larga";

struct Escenario {
    base: BaseDePrueba,
    modulo: ModuloIdentidad,
    enrutador: Router,
}

struct Respuesta {
    estado: StatusCode,
    cabeceras: axum::http::HeaderMap,
    json: Value,
}

impl Respuesta {
    /// Token de la cookie `__Host-sesion` que fijó la respuesta.
    fn cookie(&self) -> String {
        let valor = self.cabeceras[header::SET_COOKIE].to_str().unwrap();
        valor
            .strip_prefix("__Host-sesion=")
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    }
}

impl Escenario {
    async fn nuevo() -> Self {
        let base = BaseDePrueba::crear(&[
            OrigenMigraciones {
                modulo: "base",
                migraciones: &MIGRACIONES_BASE,
            },
            OrigenMigraciones {
                modulo: "identidad",
                migraciones: &identidad::MIGRACIONES,
            },
            OrigenMigraciones {
                modulo: "notificaciones",
                migraciones: &notificaciones::MIGRACIONES,
            },
            OrigenMigraciones {
                modulo: "auditoria",
                migraciones: &auditoria::MIGRACIONES,
            },
        ])
        .await;
        let reloj = Arc::new(RelojSistema);
        let modulo = ModuloIdentidad::con_postgres(DependenciasIdentidad {
            pool: base.pool.clone(),
            reloj: reloj.clone(),
            perfiles: Arc::new(SinBarberos),
            auditor: ModuloAuditoria::con_postgres(base.pool.clone(), reloj.clone()).auditor(),
            buzon: ModuloNotificaciones::con_postgres(notificaciones::DependenciasNotificaciones {
                pool: base.pool.clone(),
                reloj,
                smtp: None,
                whatsapp: &plataforma::configuracion::ConfiguracionWhatsApp {
                    simulado: false,
                    cloud_api: None,
                    webhook: None,
                },
                negocio: Arc::new(SinNegocio),
                bus: Arc::new(nucleo::eventos::BusEventos::nuevo()),
            })
            .unwrap()
            .buzon(),
            url_publica: "https://diamante.co".into(),
        });
        let enrutador = Router::new()
            .nest("/api/v1", modulo.rutas())
            .layer(Extension(Sesiones(modulo.validador_sesiones())));
        Self {
            base,
            modulo,
            enrutador,
        }
    }

    async fn pedir(
        &self,
        metodo: Method,
        ruta: &str,
        sesion: Option<&str>,
        cuerpo: Option<Value>,
    ) -> Respuesta {
        let mut peticion = Request::builder().method(metodo).uri(format!("/api/v1{ruta}"));
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

    async fn entrar(&self, correo: &str, contrasena: &str) -> Respuesta {
        self.pedir(
            Method::POST,
            "/auth/sesion",
            None,
            Some(json!({ "correo": correo, "contrasena": contrasena })),
        )
        .await
    }

    /// Token del último correo encolado para ese destinatario, leído de la cola de salida.
    async fn token_del_correo(&self, destinatario: &str) -> String {
        let variables: Value = sqlx::query_scalar(
            "SELECT variables FROM notificaciones.mensaje WHERE destinatario = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(destinatario)
        .fetch_one(&self.base.pool)
        .await
        .unwrap();
        variables["enlace"]
            .as_str()
            .unwrap()
            .split_once("#token=")
            .unwrap()
            .1
            .to_owned()
    }

    /// Crea y activa a Laura, la primera administradora, como lo haría la línea de comandos.
    async fn con_laura(&self) -> IdUsuario {
        let invitacion = self
            .modulo
            .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
            .await
            .unwrap();
        let token = self.token_del_correo("laura@diamante.co").await;
        assert_eq!(
            invitacion.enlace,
            format!("https://diamante.co/acceso/activar#token={token}")
        );
        let r = self
            .pedir(
                Method::POST,
                "/auth/activacion",
                None,
                Some(json!({ "token": token, "contrasena": CLAVE })),
            )
            .await;
        assert_eq!(r.estado, StatusCode::NO_CONTENT);
        invitacion.usuario
    }

    async fn acciones_auditadas(&self) -> Vec<String> {
        sqlx::query_scalar("SELECT accion FROM auditoria.registro ORDER BY id")
            .fetch_all(&self.base.pool)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn invitacion_activacion_sesion_y_cierre() {
    let e = Escenario::nuevo().await;
    let laura = e.con_laura().await;

    // El enlace es de un solo uso.
    let token = e.token_del_correo("laura@diamante.co").await;
    let r = e
        .pedir(
            Method::POST,
            "/auth/activacion",
            None,
            Some(json!({ "token": token, "contrasena": CLAVE })),
        )
        .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::GONE, Some("enlace_vencido"))
    );

    let r = e.entrar("laura@diamante.co", "otra frase cualquiera").await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("credenciales_invalidas"))
    );

    let r = e.entrar("LAURA@diamante.co", CLAVE).await;
    assert_eq!(r.estado, StatusCode::OK);
    assert_eq!(r.json["panel"], "/admin");
    assert_eq!(r.json["usuario"]["nombre"], "Laura Gómez");
    let set_cookie = r.cabeceras[header::SET_COOKIE].to_str().unwrap();
    assert!(
        set_cookie.contains("HttpOnly")
            && set_cookie.contains("Secure")
            && set_cookie.contains("SameSite=Strict")
    );
    assert!(
        !set_cookie.contains("Max-Age"),
        "sin «Recordarme» es una cookie de sesión del navegador"
    );
    let sesion = r.cookie();

    let r = e.pedir(Method::GET, "/auth/sesion", Some(&sesion), None).await;
    assert_eq!(
        (r.estado, r.json["usuario"]["id"].as_i64()),
        (StatusCode::OK, Some(laura.0))
    );

    // En la base solo queda la huella del token, nunca el token.
    let guardados: i64 =
        sqlx::query_scalar("SELECT count(*) FROM identidad.sesion WHERE token_hash = $1::bytea")
            .bind(sesion.as_bytes())
            .fetch_one(&e.base.pool)
            .await
            .unwrap();
    assert_eq!(guardados, 0);

    let r = e.pedir(Method::DELETE, "/auth/sesion", Some(&sesion), None).await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    assert!(
        r.cabeceras[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let r = e.pedir(Method::GET, "/auth/sesion", Some(&sesion), None).await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);

    assert_eq!(
        e.acciones_auditadas().await,
        ["invitar_administrador", "activar_cuenta"]
    );
    e.base.eliminar().await;
}

#[tokio::test]
async fn recuperacion_de_contrasena_cierra_las_sesiones_y_el_enlace_no_se_reutiliza() {
    let e = Escenario::nuevo().await;
    e.con_laura().await;
    let sesion = e.entrar("laura@diamante.co", CLAVE).await.cookie();

    // Misma respuesta exista o no la cuenta.
    for correo in ["nadie@diamante.co", "laura@diamante.co"] {
        let r = e
            .pedir(
                Method::POST,
                "/auth/recuperacion",
                None,
                Some(json!({ "correo": correo })),
            )
            .await;
        assert_eq!(r.estado, StatusCode::ACCEPTED);
    }
    let pendientes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notificaciones.mensaje WHERE plantilla = 'recuperacion_contrasena' AND contiene_secreto",
    )
    .fetch_one(&e.base.pool)
    .await
    .unwrap();
    assert_eq!(pendientes, 1);

    let token = e.token_del_correo("laura@diamante.co").await;
    let nueva = json!({ "token": token, "contrasena": "una frase nueva y larga" });
    let r = e
        .pedir(
            Method::POST,
            "/auth/recuperacion/confirmar",
            None,
            Some(nueva.clone()),
        )
        .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    let r = e
        .pedir(Method::POST, "/auth/recuperacion/confirmar", None, Some(nueva))
        .await;
    assert_eq!(r.estado, StatusCode::GONE);

    assert_eq!(
        e.pedir(Method::GET, "/auth/sesion", Some(&sesion), None)
            .await
            .estado,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.entrar("laura@diamante.co", CLAVE).await.estado,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.entrar("laura@diamante.co", "una frase nueva y larga")
            .await
            .estado,
        StatusCode::OK
    );
    e.base.eliminar().await;
}

#[tokio::test]
async fn cinco_fallos_bloquean_la_cuenta_y_el_limite_por_ip_responde_429() {
    let e = Escenario::nuevo().await;
    e.con_laura().await;
    for _ in 0..5 {
        assert_eq!(
            e.entrar("laura@diamante.co", "frase equivocada!!").await.estado,
            StatusCode::UNAUTHORIZED
        );
    }
    let bloqueada: bool = sqlx::query_scalar("SELECT bloqueado_hasta > now() FROM identidad.usuario")
        .fetch_one(&e.base.pool)
        .await
        .unwrap();
    assert!(bloqueada);
    let r = e.entrar("laura@diamante.co", CLAVE).await;
    assert_eq!(
        r.json["codigo"], "credenciales_invalidas",
        "bloqueada, con mensaje genérico"
    );

    // Van 6 intentos desde la misma IP; el 11.º supera el límite de 10 cada 15 minutos.
    for _ in 0..4 {
        e.entrar("laura@diamante.co", CLAVE).await;
    }
    let r = e.entrar("laura@diamante.co", CLAVE).await;
    assert_eq!(r.estado, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.cabeceras.contains_key(header::RETRY_AFTER));
    assert!(
        e.acciones_auditadas()
            .await
            .contains(&"bloquear_cuenta".to_owned())
    );
    e.base.eliminar().await;
}

#[tokio::test]
async fn gestion_de_administradores() {
    let e = Escenario::nuevo().await;
    let laura = e.con_laura().await;
    let sesion = e.entrar("laura@diamante.co", CLAVE).await.cookie();

    let r = e
        .pedir(
            Method::POST,
            "/administradores",
            Some(&sesion),
            Some(json!({ "correo": "camila@diamante.co", "nombre": "Camila" })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::CREATED);
    assert!(r.json.get("enlace").is_none(), "el enlace solo viaja por correo");
    let camila = r.json["id"].as_i64().unwrap();

    let r = e
        .pedir(
            Method::POST,
            "/administradores",
            Some(&sesion),
            Some(json!({ "correo": "CAMILA@diamante.co", "nombre": "Otra" })),
        )
        .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("correo_en_uso"))
    );

    let r = e
        .pedir(
            Method::POST,
            "/administradores",
            Some(&sesion),
            Some(json!({ "correo": "x", "nombre": "" })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.json["errores"].as_array().unwrap().len(), 2);

    let r = e
        .pedir(Method::GET, "/administradores", Some(&sesion), None)
        .await;
    let lista = r.json.as_array().unwrap();
    assert_eq!(lista.len(), 2);
    assert!(
        lista
            .iter()
            .any(|a| a["id"] == camila && a["pendiente_activacion"] == true)
    );

    let r = e
        .pedir(
            Method::PATCH,
            &format!("/administradores/{}", laura.0),
            Some(&sesion),
            Some(json!({ "activo": false })),
        )
        .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("no_puede_desactivarse"))
    );

    let r = e
        .pedir(
            Method::PATCH,
            &format!("/administradores/{camila}"),
            Some(&sesion),
            Some(json!({ "activo": false })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    let r = e
        .pedir(
            Method::PATCH,
            "/administradores/999",
            Some(&sesion),
            Some(json!({ "activo": false })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::NOT_FOUND);

    // Cambio de contraseña: la sesión actual sigue, las demás se cierran.
    let otra = e.entrar("laura@diamante.co", CLAVE).await.cookie();
    let r = e
        .pedir(
            Method::PUT,
            "/auth/contrasena",
            Some(&sesion),
            Some(json!({ "actual": CLAVE, "nueva": "otra frase larga y nueva" })),
        )
        .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    assert_eq!(
        e.pedir(Method::GET, "/auth/sesion", Some(&sesion), None)
            .await
            .estado,
        StatusCode::OK
    );
    assert_eq!(
        e.pedir(Method::GET, "/auth/sesion", Some(&otra), None)
            .await
            .estado,
        StatusCode::UNAUTHORIZED
    );

    let acciones = e.acciones_auditadas().await;
    assert!(acciones.contains(&"desactivar_administrador".to_owned()));
    assert!(acciones.contains(&"cambiar_contrasena".to_owned()));
    e.base.eliminar().await;
}

/// Estas pruebas no envían WhatsApp: el nombre del negocio no hace falta.
#[derive(Debug)]
struct SinNegocio;

#[async_trait::async_trait]
impl notificaciones::api::NombreDelNegocio for SinNegocio {
    async fn nombre(&self) -> Option<String> {
        None
    }
}
