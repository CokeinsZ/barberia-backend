//! WhatsApp Cloud API de punta a punta contra un Meta de mentira (un servidor HTTP local que
//! responde como la Graph API): el worker envía cada plantilla con su nombre, idioma, parámetros
//! en orden y botones; el `wamid` que devuelve Meta une el mensaje con los webhooks de estado y
//! con las respuestas a los botones.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::post;
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::pruebas_postgres::{
    CLAVE, SECRETO_APP, SITIO, boton, entrar, firmado, pedir, registro, token_del_correo, webhook,
};
use crate::{enrutador, migraciones, worker};

/// Lo que recibió Meta: número de teléfono de la URL, autorización y cuerpo.
#[derive(Clone, Default)]
struct Meta(Arc<Mutex<Vec<(String, String, Value)>>>);

impl Meta {
    fn recibidas(&self) -> Vec<(String, String, Value)> {
        self.0.lock().unwrap().clone()
    }
}

async fn mensajes(
    State(meta): State<Meta>,
    Path(numero): Path<String>,
    cabeceras: HeaderMap,
    cuerpo: String,
) -> (StatusCode, String) {
    let json: Value = serde_json::from_str(&cuerpo).unwrap();
    let destino = json["to"].as_str().unwrap_or_default().to_owned();
    let mut recibidas = meta.0.lock().unwrap();
    recibidas.push((
        numero,
        cabeceras["authorization"].to_str().unwrap().to_owned(),
        json,
    ));
    if destino.ends_with("9999") {
        return (
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": "(#131026) Message undeliverable", "code": 131_026,
                               "error_data": { "details": "Message Undeliverable." } } })
            .to_string(),
        );
    }
    let wamid = format!("wamid.meta.{}", recibidas.len());
    (
        StatusCode::OK,
        json!({ "messaging_product": "whatsapp", "contacts": [{ "input": destino, "wa_id": destino }],
                "messages": [{ "id": wamid, "message_status": "accepted" }] })
        .to_string(),
    )
}

fn parametros(cuerpo: &Value, componente: usize) -> Vec<String> {
    cuerpo["template"]["components"][componente]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p["text"]
                .as_str()
                .or_else(|| p["payload"].as_str())
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn whatsapp_cloud_api_de_punta_a_punta() {
    let meta = Meta::default();
    let servidor = Router::new()
        .route("/v26.0/{numero}/messages", post(mensajes))
        .with_state(meta.clone());
    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}/v26.0", escucha.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(escucha, servidor).await.unwrap() });

    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_LIMITES_DE_USO" => Some("false".into()),
        "DIAMANTE_WHATSAPP_TOKEN" => Some("token-de-meta".into()),
        "DIAMANTE_WHATSAPP_ID_NUMERO" => Some("1234567890".into()),
        "DIAMANTE_WHATSAPP_API" => Some(api.clone()),
        "DIAMANTE_WHATSAPP_SECRETO_APP" => Some(SECRETO_APP.into()),
        "DIAMANTE_WHATSAPP_TOKEN_VERIFICACION" => Some("token-de-verificacion".into()),
        _ => None,
    })
    .unwrap();
    assert!(
        !configuracion.whatsapp.simulado,
        "con credenciales de Meta no se simula"
    );
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(nucleo::RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);
    composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
        .await
        .unwrap();
    let token = token_del_correo(&base, "laura@diamante.co").await;
    pedir(
        &app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let crear = |ruta: &'static str, cuerpo: Value| {
        let (app, admin) = (app.clone(), admin.clone());
        async move {
            let r = pedir(
                &app,
                Method::POST,
                &format!("/api/v1{ruta}"),
                Some(&admin),
                Some(cuerpo),
            )
            .await;
            assert!(r.estado.is_success(), "{ruta}: {}", r.json);
            r.json
        }
    };
    // El nombre comercial que va en las plantillas: el configurado, no uno fijo.
    let negocio = pedir(&app, Method::GET, "/api/v1/negocio", Some(&admin), None)
        .await
        .json;
    let r = pedir(
        &app,
        Method::PUT,
        "/api/v1/negocio",
        Some(&admin),
        Some(json!({ "nombre_comercial": "Barbería del Centro", "nit": negocio["nit"], "direccion": negocio["direccion"],
                     "zona_referencia": negocio["zona_referencia"], "whatsapp": negocio["whatsapp"],
                     "instagram": negocio["instagram"], "porcentaje_comision_defecto": negocio["porcentaje_comision_defecto"] })),
    )
    .await;
    assert!(r.estado.is_success(), "{}", r.json);
    let corte = crear(
        "/servicios",
        json!({ "categoria_id": 1, "nombre": "Corte clásico", "descripcion": "Corte.", "precio": 35_000, "duracion_min": 30 }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let santiago = crear(
        "/barberos",
        registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![json!(corte)],
        ),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let hoy = chrono::Utc::now()
        .with_timezone(&nucleo::ZONA_HORARIA)
        .date_naive();
    let dia = (8..15)
        .map(|n| hoy + chrono::Days::new(n))
        .find(|d| chrono::Datelike::weekday(d) != chrono::Weekday::Sun)
        .unwrap();
    let cita = crear(
        "/agenda/citas",
        json!({ "cliente_nuevo": { "nombre": "Andrea Gómez", "celular": "3001112233" }, "servicio_id": corte,
                "barbero_id": santiago, "inicio": format!("{dia}T09:00:00-05:00") }),
    )
    .await;
    let cita_id = cita["id"].as_i64().unwrap();

    // 1. La confirmación sale por Meta con la plantilla, el idioma y los parámetros en orden.
    let pasada = worker::pasada(&composicion).await;
    assert!(pasada.errores.is_empty(), "{:?}", pasada.errores);
    assert_eq!(pasada.whatsapp.enviados, 1, "{pasada:?}");
    let recibidas = meta.recibidas();
    let (numero, autorizacion, cuerpo) = &recibidas[0];
    assert_eq!(
        (numero.as_str(), autorizacion.as_str()),
        ("1234567890", "Bearer token-de-meta")
    );
    assert_eq!(cuerpo["to"], "573001112233");
    assert_eq!(cuerpo["template"]["name"], "cita_confirmada");
    assert_eq!(cuerpo["template"]["language"]["code"], "es_CO");
    let p = parametros(cuerpo, 0);
    assert_eq!(p.len(), 7, "{p:?}");
    assert_eq!(
        p[..4],
        // El saludo lleva solo el primer nombre.
        ["Andrea", "Barbería del Centro", "Corte clásico", "Santiago Ruiz"]
    );
    assert!(
        p[4].contains(&format!(" {} de ", chrono::Datelike::day(&dia))),
        "fecha: {}",
        p[4]
    );
    assert_eq!(
        p[5], "9:00 a. m",
        "la hora sin su punto: en la plantilla la sigue un punto"
    );
    assert!(p[6].starts_with(&format!("{SITIO}/cita/")), "{}", p[6]);
    let (estado, wamid, variables): (String, String, Value) = sqlx::query_as(
        "SELECT estado::text, id_proveedor, variables FROM notificaciones.mensaje
         WHERE canal = 'whatsapp' AND plantilla = 'cita_confirmada'",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert_eq!((estado.as_str(), wamid.as_str()), ("enviado", "wamid.meta.1"));
    assert!(
        variables.as_object().is_none_or(serde_json::Map::is_empty),
        "el enlace de gestión es secreto: se borra al enviar ({variables})"
    );

    // 2. Meta avisa que se entregó (webhook firmado con el wamid).
    let entregado = json!({ "object": "whatsapp_business_account", "entry": [{ "id": "WABA", "changes": [{ "field": "messages",
        "value": { "messaging_product": "whatsapp",
                   "statuses": [{ "id": wamid, "status": "delivered", "timestamp": "1763500000", "recipient_id": "573001112233" }] } }] }] });
    assert_eq!(
        webhook(&app, &entregado, firmado(&entregado)).await.estado,
        StatusCode::OK
    );
    worker::pasada(&composicion).await;
    let estado: String =
        sqlx::query_scalar("SELECT estado::text FROM notificaciones.mensaje WHERE id_proveedor = $1")
            .bind(&wamid)
            .fetch_one(&base.pool)
            .await
            .unwrap();
    assert_eq!(estado, "entregado");

    // 3. Recordatorio manual: botones con su carga; «Confirmo» confirma la cita.
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{cita_id}/recordatorio"),
        Some(&admin),
        None,
    )
    .await;
    assert!(r.estado.is_success(), "{}", r.json);
    worker::pasada(&composicion).await;
    let recibidas = meta.recibidas();
    let (_, _, recordatorio) = recibidas.last().unwrap();
    assert_eq!(recordatorio["template"]["name"], "recordatorio_cita");
    let p = parametros(recordatorio, 0);
    assert_eq!(
        (p.len(), p[0].as_str(), p[1].as_str(), p[4].as_str()),
        (5, "Andrea", "Barbería del Centro", "Santiago Ruiz")
    );
    assert_eq!(parametros(recordatorio, 1), ["confirmar"]);
    assert_eq!(parametros(recordatorio, 2), ["cancelar"]);
    let wamid_recordatorio = format!("wamid.meta.{}", recibidas.len());
    let respuesta = boton(&wamid_recordatorio, "confirmar");
    assert_eq!(
        webhook(&app, &respuesta, firmado(&respuesta)).await.estado,
        StatusCode::OK
    );
    worker::pasada(&composicion).await;
    let detalle = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/citas/{cita_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(detalle.json["estado"], "confirmada", "{}", detalle.json);

    // 4. El código de verificación: en el cuerpo y en el botón «Copiar código».
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": "3004445566" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::ACCEPTED, "{}", r.json);
    worker::pasada(&composicion).await;
    let recibidas = meta.recibidas();
    let (_, _, codigo) = recibidas.last().unwrap();
    assert_eq!(codigo["template"]["name"], "codigo_verificacion");
    let en_cuerpo = parametros(codigo, 0);
    assert_eq!(en_cuerpo.len(), 1);
    assert_eq!(en_cuerpo[0].len(), 4);
    assert_eq!(codigo["template"]["components"][1]["sub_type"], "url");
    assert_eq!(parametros(codigo, 1), en_cuerpo, "el botón copia el mismo código");

    // 5. Un número sin WhatsApp: queda fallido de una vez y la administradora recibe el aviso.
    crear(
        "/agenda/citas",
        json!({ "cliente_nuevo": { "nombre": "Bruno Díaz", "celular": "3001119999" }, "servicio_id": corte,
                "barbero_id": santiago, "inicio": format!("{dia}T10:00:00-05:00") }),
    )
    .await;
    let pasada = worker::pasada(&composicion).await;
    assert_eq!(
        (pasada.whatsapp.enviados, pasada.whatsapp.fallidos),
        (0, 1),
        "{pasada:?}"
    );
    let (estado, error): (String, String) = sqlx::query_as(
        "SELECT estado::text, ultimo_error FROM notificaciones.mensaje WHERE destinatario = '3001119999'",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert_eq!(estado, "fallido");
    assert!(error.starts_with("Meta 131026"), "{error}");
    let avisos = pedir(
        &app,
        Method::GET,
        "/api/v1/avisos?no_leidos=true",
        Some(&admin),
        None,
    )
    .await;
    assert!(
        avisos.json["datos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["tipo"] == "mensaje_fallido" && a["titulo"].as_str().unwrap().contains("Bruno")),
        "{}",
        avisos.json
    );
    // La vigilancia sabe que sale por Meta (la batería de preproducción se niega a correr así).
    let estado = pedir(&app, Method::GET, "/api/v1/sistema/estado", Some(&admin), None).await;
    assert_eq!(
        estado.json["cola"]["proveedor_whatsapp"], "meta",
        "{}",
        estado.json
    );
    assert_eq!(estado.json["cola"]["whatsapp"]["configurado"], true);
    // Nadie vio el token: ni en la configuración ni en los errores guardados.
    assert!(!format!("{configuracion:?}").contains("token-de-meta"));
    base.eliminar().await;
}
