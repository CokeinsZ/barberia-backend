//! Concurrencia sobre PostgreSQL real (RNF-INT-01, RN-19): peticiones simultáneas de verdad
//! (runtime de varios hilos y transacciones en paralelo) contra el enrutador completo.
//!
//! - Ocho reservas a la misma hora con el mismo barbero: una sola queda; las demás responden
//!   `409 conflicto_horario` y no dejan clientes huérfanos (todo va en la misma transacción).
//! - Ocho reservas con «Cualquier barbero» a la misma hora y dos barberos libres: exactamente
//!   dos citas, una por barbero. Los bloqueos mutuos de la restricción de exclusión se
//!   reintentan (antes respondían `500`).
//! - Diez ventas a la vez: tiquetes consecutivos, sin huecos ni repetidos.
//! - La misma cita cobrada cinco veces a la vez: una sola venta.
//! - Reserva en línea por la misma hora, celulares repetidos, ventas simultáneas al mismo
//!   cliente (métricas exactas), anulaciones y cierres de caja dobles, transiciones cruzadas de
//!   una cita, tres pasadas del worker a la vez y contraseñas equivocadas en paralelo.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};
use tokio::task::JoinSet;

use crate::composicion::Composicion;
use crate::pruebas_postgres::{
    CLAVE, Respuesta, SITIO, entrar, mover_reloj, pedir, registro, token_del_correo,
};
use crate::{enrutador, migraciones};

/// Lanza las peticiones a la vez y devuelve las respuestas en el orden en que terminan.
async fn a_la_vez(app: &Router, sesion: &str, ruta: &str, cuerpos: Vec<Value>) -> Vec<Respuesta> {
    let mut tareas = JoinSet::new();
    for cuerpo in cuerpos {
        let (app, sesion, ruta) = (app.clone(), sesion.to_owned(), ruta.to_owned());
        tareas.spawn(async move { pedir(&app, Method::POST, &ruta, Some(&sesion), Some(cuerpo)).await });
    }
    let mut respuestas = Vec::new();
    while let Some(r) = tareas.join_next().await {
        respuestas.push(r.unwrap());
    }
    respuestas
}

fn por_estado(respuestas: &[Respuesta]) -> BTreeMap<u16, usize> {
    let mut conteo = BTreeMap::new();
    for r in respuestas {
        *conteo.entry(r.estado.as_u16()).or_insert(0) += 1;
    }
    conteo
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::too_many_lines)]
async fn reservas_y_ventas_simultaneas() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_PRUEBAS_E2E" => Some("true".into()),
        _ => None,
    })
    .unwrap();
    let composicion = Composicion::con_reloj_de_pruebas(
        base.pool.clone(),
        Arc::new(nucleo::RelojDePruebas::default()),
        &configuracion,
    )
    .unwrap();
    let app = enrutador::construir(&composicion);
    // Miércoles 18 de noviembre de 2026, 8:00 a. m.
    mover_reloj(&app, "2026-11-18T08:00:00-05:00").await;
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
            r.json["id"].as_i64().unwrap()
        }
    };
    let corte = crear(
        "/servicios",
        json!({ "categoria_id": 1, "nombre": "Corte clásico", "descripcion": "Corte.", "precio": 35000, "duracion_min": 30 }),
    )
    .await;
    let santiago = crear(
        "/barberos",
        registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![json!(corte)],
        ),
    )
    .await;
    let mateo = crear(
        "/barberos",
        registro("Mateo", "1020304051", "mateo@diamante.co", vec![json!(corte)]),
    )
    .await;
    let cita = |i: usize, barbero: Option<i64>, inicio: &str| {
        json!({ "cliente_nuevo": { "nombre": format!("Cliente {i}"), "celular": format!("31000000{i:02}") },
                "servicio_id": corte, "barbero_id": barbero, "inicio": inicio })
    };

    // 1. Ocho reservas a la misma hora con Santiago (miércoles siguiente, 10:00).
    let cuerpos = (1..=8)
        .map(|i| cita(i, Some(santiago), "2026-11-25T10:00:00-05:00"))
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/agenda/citas", cuerpos).await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(201, 1), (409, 7)]),
        "{:?}",
        respuestas.iter().map(|r| &r.json).collect::<Vec<_>>()
    );
    assert!(
        respuestas
            .iter()
            .filter(|r| r.estado == StatusCode::CONFLICT)
            .all(|r| r.json["codigo"] == "conflicto_horario")
    );
    // Los clientes de las reservas que perdieron no quedaron registrados (misma transacción).
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/clientes?q=Cliente",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["total"], 1, "{}", r.json);

    // 2. «Cualquier barbero» ×8 a las 11:00, con Santiago y Mateo libres.
    let cuerpos = (11..=18)
        .map(|i| cita(i, None, "2026-11-25T11:00:00-05:00"))
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/agenda/citas", cuerpos).await;
    let creadas: Vec<i64> = respuestas
        .iter()
        .filter(|r| r.estado == StatusCode::CREATED)
        .map(|r| r.json["barbero"]["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        creadas.len(),
        2,
        "una por barbero (los choques se reintentan): {:?}",
        respuestas.iter().map(|r| (r.estado, &r.json)).collect::<Vec<_>>()
    );
    assert!(
        creadas.iter().all(|b| [santiago, mateo].contains(b))
            && (creadas.len() < 2 || creadas[0] != creadas[1]),
        "nunca dos citas al mismo barbero: {creadas:?}"
    );
    assert!(
        respuestas
            .iter()
            .filter(|r| r.estado != StatusCode::CREATED)
            .all(|r| r.estado == StatusCode::CONFLICT),
        "{:?}",
        respuestas
            .iter()
            .map(|r| (r.estado, r.json["codigo"].clone(), r.json["title"].clone()))
            .collect::<Vec<_>>()
    );
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/agenda/citas?desde=2026-11-25&hasta=2026-11-25",
        Some(&admin),
        None,
    )
    .await;
    let a_las_11 = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["inicio"].as_str().unwrap().contains("T11:00"))
        .count();
    assert_eq!(a_las_11, creadas.len(), "{}", r.json);

    // 3. Diez ventas a la vez: tiquetes consecutivos sin huecos ni repetidos (RN-19).
    let catalogos = pedir(&app, Method::GET, "/api/v1/ventas/catalogos", Some(&admin), None).await;
    let efectivo = catalogos.json["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let venta = |barbero: i64| {
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": barbero, "servicio_ids": [corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] })
    };
    let cuerpos = (0..10)
        .map(|i| venta(if i % 2 == 0 { santiago } else { mateo }))
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/ventas", cuerpos).await;
    assert_eq!(por_estado(&respuestas), BTreeMap::from([(201, 10)]));
    let mut tiquetes: Vec<i64> = respuestas
        .iter()
        .map(|r| r.json["numero_ticket"].as_i64().unwrap())
        .collect();
    tiquetes.sort_unstable();
    assert_eq!(tiquetes, (1..=10).collect::<Vec<_>>());

    // 4. La misma cita de hoy cobrada cinco veces a la vez: una sola venta.
    let cita_hoy = crear(
        "/agenda/citas",
        cita(30, Some(santiago), "2026-11-18T10:00:00-05:00"),
    )
    .await;
    let cuerpos = (0..5)
        .map(|_| {
            json!({ "cita_id": cita_hoy, "servicio_ids": [corte], "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] })
        })
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/ventas", cuerpos).await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(201, 1), (409, 4)]),
        "{:?}",
        respuestas.iter().map(|r| &r.json).collect::<Vec<_>>()
    );
    assert!(
        respuestas
            .iter()
            .filter(|r| r.estado == StatusCode::CONFLICT)
            .all(|r| r.json["codigo"] == "cita_ya_cobrada"),
        "{:?}",
        respuestas.iter().map(|r| &r.json).collect::<Vec<_>>()
    );
    let ganadora = respuestas
        .iter()
        .find(|r| r.estado == StatusCode::CREATED)
        .unwrap();
    assert_eq!(
        ganadora.json["numero_ticket"], 11,
        "sigue la numeración sin huecos"
    );
    let r = pedir(&app, Method::GET, "/api/v1/ventas/caja", Some(&admin), None).await;
    assert_eq!(
        (r.json["numero_ventas"].as_u64(), r.json["total_cobrado"].as_i64()),
        (Some(11), Some(11 * 35_000)),
        "{}",
        r.json
    );
}

/// Lanza peticiones distintas a la vez (método, ruta, sesión, cuerpo).
async fn mezcla(
    app: &Router,
    peticiones: Vec<(Method, String, Option<String>, Option<Value>)>,
) -> Vec<Respuesta> {
    let mut tareas = JoinSet::new();
    for (metodo, ruta, sesion, cuerpo) in peticiones {
        let app = app.clone();
        tareas.spawn(async move { pedir(&app, metodo, &ruta, sesion.as_deref(), cuerpo).await });
    }
    let mut respuestas = Vec::new();
    while let Some(r) = tareas.join_next().await {
        respuestas.push(r.unwrap());
    }
    respuestas
}

fn codigos(respuestas: &[Respuesta]) -> Vec<(u16, String)> {
    let mut v: Vec<_> = respuestas
        .iter()
        .map(|r| {
            (
                r.estado.as_u16(),
                r.json["codigo"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    v.sort();
    v
}

/// Más carreras: reserva en línea, celulares repetidos, la proyección de métricas del cliente,
/// anulaciones y cierres dobles, transiciones cruzadas de una cita, dos workers a la vez y
/// contraseñas equivocadas en paralelo. Ninguna respuesta puede ser un `500`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::too_many_lines)]
async fn carreras_de_clientes_ventas_agenda_y_worker() {
    use crate::pruebas_postgres::verificar;

    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_PRUEBAS_E2E" => Some("true".into()),
        _ => None,
    })
    .unwrap();
    let composicion = Composicion::con_reloj_de_pruebas(
        base.pool.clone(),
        Arc::new(nucleo::RelojDePruebas::default()),
        &configuracion,
    )
    .unwrap();
    let app = enrutador::construir(&composicion);
    mover_reloj(&app, "2026-11-18T08:00:00-05:00").await;
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
    let politica = crear(
        "/negocio/politicas",
        json!({ "tipo": "tratamiento_datos", "version": "1.0", "url": "https://diamante.co/politicas/datos" }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let corte = crear(
        "/servicios",
        json!({ "categoria_id": 1, "nombre": "Corte clásico", "descripcion": "Corte.", "precio": 35000, "duracion_min": 30 }),
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
    let todas = |r: &[Respuesta]| r.iter().all(|r| !r.estado.is_server_error());

    // 1. Reserva en línea: seis clientes verificados toman a la vez la misma hora. Una gana; los
    //    demás reciben 409 y su comprobante sigue sirviendo para otra hora.
    let mut comprobantes = Vec::new();
    for i in 1..=6 {
        let v = verificar(&app, &base, &format!("31100000{i:02}")).await;
        comprobantes.push(v["comprobante"].as_str().unwrap().to_owned());
    }
    let reserva = |comprobante: &str, inicio: &str, i: usize| {
        json!({ "comprobante": comprobante, "servicio_id": corte, "barbero_id": santiago, "inicio": inicio,
                "cliente": { "nombre": format!("Cliente En Línea {i}"), "correo": null, "fecha_nacimiento": null },
                "autoriza_tratamiento_datos": true, "politica_version_id": politica })
    };
    let peticiones = comprobantes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                Method::POST,
                "/api/v1/publico/citas".to_owned(),
                None,
                Some(reserva(c, "2026-11-25T10:00:00-05:00", i)),
            )
        })
        .collect();
    let respuestas = mezcla(&app, peticiones).await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(201, 1), (409, 5)]),
        "{:?}",
        codigos(&respuestas)
    );
    // Cada comprobante en una hora distinta: los cinco que perdieron siguen sirviendo y el del
    // ganador ya se usó.
    let mut otra_vez = Vec::new();
    for (i, c) in comprobantes.iter().enumerate() {
        let inicio = format!(
            "2026-11-25T{}:{}:00-05:00",
            11 + i / 2,
            if i % 2 == 0 { "00" } else { "30" }
        );
        let r = pedir(
            &app,
            Method::POST,
            "/api/v1/publico/citas",
            None,
            Some(reserva(c, &inicio, i)),
        )
        .await;
        otra_vez.push(r);
    }
    assert_eq!(
        por_estado(&otra_vez).get(&201),
        Some(&5),
        "los comprobantes perdedores siguen sirviendo: {:?}",
        codigos(&otra_vez)
    );
    assert!(todas(&otra_vez), "{:?}", codigos(&otra_vez));

    // 2. El mismo celular registrado cinco veces a la vez: un cliente, cuatro 409 celular_en_uso.
    let cuerpos = (0..5)
        .map(|i| json!({ "nombre": format!("Pablo Vélez {i}"), "celular": "3105559917" }))
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/clientes", cuerpos).await;
    assert_eq!(
        codigos(&respuestas),
        vec![
            (201, String::new()),
            (409, "celular_en_uso".into()),
            (409, "celular_en_uso".into()),
            (409, "celular_en_uso".into()),
            (409, "celular_en_uso".into())
        ]
    );
    let pablo = respuestas
        .iter()
        .find(|r| r.estado == StatusCode::CREATED)
        .unwrap()
        .json["id"]
        .as_i64()
        .unwrap();

    // 3. Diez ventas a la vez al mismo cliente: la ficha suma exactamente diez visitas (ninguna
    //    actualización de la proyección se pierde).
    let catalogos = pedir(&app, Method::GET, "/api/v1/ventas/catalogos", Some(&admin), None).await;
    let efectivo = catalogos.json["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let venta = json!({ "cliente": { "tipo": "registrado", "id": pablo }, "barbero_id": santiago, "servicio_ids": [corte],
                        "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] });
    let respuestas = a_la_vez(&app, &admin, "/api/v1/ventas", vec![venta; 10]).await;
    assert_eq!(por_estado(&respuestas), BTreeMap::from([(201, 10)]));
    let ficha = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/clientes/{pablo}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (
            ficha.json["visitas"].as_i64(),
            ficha.json["total_gastado"].as_i64()
        ),
        (Some(10), Some(350_000)),
        "{}",
        ficha.json
    );

    // 4. La misma venta anulada cinco veces a la vez: una anulación y la visita se resta una vez.
    let venta_id = respuestas[0].json["id"].as_i64().unwrap();
    let cuerpos = (0..5)
        .map(|_| json!({ "motivo": "Se registró dos veces" }))
        .collect();
    let respuestas = a_la_vez(
        &app,
        &admin,
        &format!("/api/v1/ventas/{venta_id}/anulacion"),
        cuerpos,
    )
    .await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(200, 1), (409, 4)]),
        "{:?}",
        codigos(&respuestas)
    );
    let ficha = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/clientes/{pablo}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (
            ficha.json["visitas"].as_i64(),
            ficha.json["total_gastado"].as_i64()
        ),
        (Some(9), Some(315_000)),
        "{}",
        ficha.json
    );

    // 5. Cuatro cierres de caja del mismo día a la vez: uno queda.
    let cuerpos = (0..4)
        .map(|_| json!({ "fecha": "2026-11-18", "contado": [{ "metodo_pago_id": efectivo, "monto": 315000 }] }))
        .collect();
    let respuestas = a_la_vez(&app, &admin, "/api/v1/ventas/cierres", cuerpos).await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(201, 1), (409, 3)]),
        "{:?}",
        codigos(&respuestas)
    );
    assert!(
        respuestas
            .iter()
            .filter(|r| r.estado == StatusCode::CONFLICT)
            .all(|r| r.json["codigo"] == "cierre_existente")
    );

    // 6. Transiciones cruzadas de una misma cita (cancelar, confirmar, iniciar y recordar a la
    //    vez): sin errores internos y la cita queda en uno solo de los estados posibles, con su
    //    historial coherente.
    let cita = crear(
        "/agenda/citas",
        json!({ "cliente_id": pablo, "servicio_id": corte, "barbero_id": santiago, "inicio": "2026-11-18T15:00:00-05:00" }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let ruta = |accion: &str| format!("/api/v1/agenda/citas/{cita}/{accion}");
    let peticiones = vec![
        (
            Method::POST,
            ruta("cancelar"),
            Some(admin.clone()),
            Some(json!({ "motivo": "solicitud_cliente" })),
        ),
        (Method::POST, ruta("confirmar"), Some(admin.clone()), None),
        (Method::POST, ruta("iniciar"), Some(admin.clone()), None),
        (Method::POST, ruta("confirmar"), Some(admin.clone()), None),
    ];
    let respuestas = mezcla(&app, peticiones).await;
    assert!(todas(&respuestas), "{:?}", codigos(&respuestas));
    let detalle = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/citas/{cita}"),
        Some(&admin),
        None,
    )
    .await;
    let estado = detalle.json["estado"].as_str().unwrap().to_owned();
    assert!(
        ["cancelada", "confirmada", "en_curso"].contains(&estado.as_str()),
        "{estado}"
    );
    let eventos: Vec<&str> = detalle.json["historial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["evento"].as_str().unwrap())
        .collect();
    assert_eq!(
        eventos.iter().filter(|e| **e == "cancelada").count(),
        usize::from(estado == "cancelada")
    );
    if estado == "cancelada" {
        assert_eq!(
            eventos.last(),
            Some(&"cancelada"),
            "nada ocurre después de cancelar: {eventos:?}"
        );
    }

    // 7. Tres pasadas del worker a la vez: cada WhatsApp sale una sola vez (SKIP LOCKED) y el
    //    planificador no duplica trabajos.
    let pendientes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notificaciones.mensaje WHERE canal = 'whatsapp' AND estado = 'pendiente'",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert!(pendientes >= 3, "hay confirmaciones por enviar: {pendientes}");
    let mut tareas = JoinSet::new();
    for _ in 0..3 {
        let c = composicion.clone();
        tareas.spawn(async move { crate::worker::pasada(&c).await });
    }
    let mut enviados = 0;
    while let Some(r) = tareas.join_next().await {
        let resumen = r.unwrap();
        assert!(resumen.errores.is_empty(), "{:?}", resumen.errores);
        enviados += resumen.whatsapp.enviados;
    }
    let (sin_enviar, repetidos): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE estado = 'pendiente'), count(*) FILTER (WHERE intentos > 1)
         FROM notificaciones.mensaje WHERE canal = 'whatsapp'",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert_eq!(
        (sin_enviar, repetidos, i64::try_from(enviados).unwrap()),
        (0, 0, pendientes),
        "cada mensaje una sola vez"
    );
    let duplicados: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT clave_unica FROM trabajos.trabajo WHERE clave_unica IS NOT NULL
         GROUP BY clave_unica HAVING count(*) > 1) d",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert_eq!(duplicados, 0);

    // 8. Seis contraseñas equivocadas a la vez: ningún fallo se pierde y la cuenta queda
    //    bloqueada (la fila del usuario se toma con FOR UPDATE).
    let cuerpos = (0..6)
        .map(|_| json!({ "correo": "laura@diamante.co", "contrasena": "equivocada" }))
        .collect::<Vec<_>>();
    let peticiones = cuerpos
        .into_iter()
        .map(|c| (Method::POST, "/api/v1/auth/sesion".to_owned(), None, Some(c)))
        .collect();
    let respuestas = mezcla(&app, peticiones).await;
    assert_eq!(
        por_estado(&respuestas),
        BTreeMap::from([(401, 6)]),
        "{:?}",
        codigos(&respuestas)
    );
    let (intentos, bloqueada): (i16, bool) = sqlx::query_as(
        "SELECT intentos_fallidos, bloqueado_hasta IS NOT NULL FROM identidad.usuario WHERE correo = 'laura@diamante.co'",
    )
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert!(
        bloqueada,
        "la cuenta debe quedar bloqueada (intentos: {intentos})"
    );
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/auth/sesion",
        None,
        Some(json!({ "correo": "laura@diamante.co", "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(
        r.estado,
        StatusCode::UNAUTHORIZED,
        "bloqueada, ni la contraseña correcta entra"
    );
}
