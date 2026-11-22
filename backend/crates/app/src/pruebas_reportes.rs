//! Reportes de punta a punta (fase 6, E2E-REP-01 a 04): dashboard, resumen, liquidación por
//! barbero, clientes, tendencias, alertas y PDF, sobre PostgreSQL real con todas las
//! migraciones. Gotenberg es un servidor local de mentira que devuelve lo que recibe (así se
//! revisa qué se le envió) o falla cuando se le pide.
//!
//! El escenario se siembra por la API moviendo el reloj de pruebas: agosto (la última visita de
//! Rosa), octubre (primera visita de Pablo, con Santiago al 40 %), el miércoles 11 de noviembre
//! y hoy, miércoles 18 de noviembre de 2026, a las 10:30 a. m.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, header};
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::pruebas_postgres::{
    CLAVE, SITIO, entrar, mover_reloj, pedir, pedir_bytes, registro, token_del_correo,
};
use crate::{enrutador, migraciones};

/// Gotenberg de mentira: responde un «PDF» con el multipart recibido o un 500 si `fallar`.
async fn gotenberg_falso() -> (String, Arc<AtomicBool>) {
    async fn convertir(State(fallar): State<Arc<AtomicBool>>, cuerpo: Bytes) -> (StatusCode, Vec<u8>) {
        if fallar.load(Ordering::SeqCst) {
            return (StatusCode::INTERNAL_SERVER_ERROR, b"chromium se cayo".to_vec());
        }
        let mut pdf = b"%PDF-1.7\n".to_vec();
        pdf.extend_from_slice(&cuerpo);
        (StatusCode::OK, pdf)
    }
    let fallar = Arc::new(AtomicBool::new(false));
    let rutas = Router::new()
        .route("/forms/chromium/convert/html", axum::routing::post(convertir))
        .with_state(fallar.clone());
    let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", escucha.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(escucha, rutas).await.unwrap() });
    (url, fallar)
}

async fn activar(app: &Router, base: &BaseDePrueba, correo: &str) {
    let token = token_del_correo(base, correo).await;
    let r = pedir(
        app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT, "{}", r.json);
}

async fn ver(app: &Router, ruta: &str, sesion: &str) -> Value {
    let r = pedir(app, Method::GET, ruta, Some(sesion), None).await;
    assert_eq!(r.estado, StatusCode::OK, "{ruta}: {}", r.json);
    r.json
}

async fn crear(app: &Router, ruta: &str, sesion: &str, cuerpo: Value) -> Value {
    let r = pedir(app, Method::POST, ruta, Some(sesion), Some(cuerpo)).await;
    assert_eq!(r.estado, StatusCode::CREATED, "{ruta}: {}", r.json);
    r.json
}

/// Cita desde el panel para un cliente registrado (`cliente` es su id) o nuevo (un objeto).
async fn agendar(
    app: &Router,
    sesion: &str,
    cliente: Value,
    servicio: i64,
    barbero: i64,
    inicio: &str,
) -> i64 {
    let mut cuerpo = json!({ "servicio_id": servicio, "barbero_id": barbero, "inicio": inicio });
    if cliente.is_number() {
        cuerpo["cliente_id"] = cliente;
    } else {
        cuerpo["cliente_nuevo"] = cliente;
    }
    let r = pedir(
        app,
        Method::POST,
        "/api/v1/agenda/citas",
        Some(sesion),
        Some(cuerpo.clone()),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{cuerpo}: {}", r.json);
    id(&r.json)
}

async fn descargar(app: &Router, ruta: &str, sesion: Option<&str>) -> (StatusCode, HeaderMap, Vec<u8>) {
    pedir_bytes(app, Method::GET, ruta, sesion, "", Vec::new()).await
}

fn id(v: &Value) -> i64 {
    v["id"].as_i64().unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn reportes_de_punta_a_punta() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let (gotenberg, fallar) = gotenberg_falso().await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_PRUEBAS_E2E" => Some("true".into()),
        "DIAMANTE_GOTENBERG_URL" => Some(gotenberg.clone()),
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

    // ── Lunes 3 de agosto: equipo, catálogo y la última visita de Rosa.
    mover_reloj(&app, "2026-08-03T09:00:00-05:00").await;
    composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
        .await
        .unwrap();
    activar(&app, &base, "laura@diamante.co").await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let r = pedir(
        &app,
        Method::PUT,
        "/api/v1/negocio",
        Some(&admin),
        Some(json!({ "nombre_comercial": "Diamante Barbería", "nit": "900.123.456-7", "porcentaje_comision_defecto": 50 })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let servicio = |nombre: &str, precio: i64| json!({ "categoria_id": 1, "nombre": nombre, "descripcion": "Servicio.", "precio": precio, "duracion_min": 30 });
    let corte = id(&crear(
        &app,
        "/api/v1/servicios",
        &admin,
        servicio("Corte clásico", 35_000),
    )
    .await);
    let cejas = id(&crear(
        &app,
        "/api/v1/servicios",
        &admin,
        servicio("Diseño de cejas", 10_000),
    )
    .await);
    let mut datos_santiago = registro(
        "Santiago",
        "1020304050",
        "santiago@diamante.co",
        vec![json!(corte), json!(cejas)],
    );
    datos_santiago["porcentaje_comision"] = json!(40);
    let santiago = id(&crear(&app, "/api/v1/barberos", &admin, datos_santiago.clone()).await);
    let mateo = id(&crear(
        &app,
        "/api/v1/barberos",
        &admin,
        registro(
            "Mateo",
            "1020304051",
            "mateo@diamante.co",
            vec![json!(corte), json!(cejas)],
        ),
    )
    .await);
    let daniel = id(&crear(
        &app,
        "/api/v1/barberos",
        &admin,
        registro("Daniel", "1020304052", "daniel@diamante.co", vec![json!(corte)]),
    )
    .await);
    activar(&app, &base, "santiago@diamante.co").await;

    let catalogos = ver(&app, "/api/v1/ventas/catalogos", &admin).await;
    let metodo = |codigo: &str| {
        catalogos["metodos_pago"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["codigo"] == codigo)
            .map(id)
            .unwrap()
    };
    let (efectivo, nequi) = (metodo("efectivo"), metodo("nequi"));
    let diez = catalogos["descuentos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["nombre"] == "10 %")
        .map(id)
        .unwrap();
    let motivo = id(&catalogos["motivos_descuento"][0]);

    let cliente = |nombre: &str, celular: &str, nacimiento: &str| json!({ "nombre": nombre, "celular": celular, "fecha_nacimiento": nacimiento });
    let rosa = id(&crear(
        &app,
        "/api/v1/clientes",
        &admin,
        cliente("Rosa Peña", "3001110001", "1990-11-20"),
    )
    .await);
    let pablo = id(&crear(
        &app,
        "/api/v1/clientes",
        &admin,
        cliente("Pablo Vélez", "3105559917", "1992-11-18"),
    )
    .await);
    crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cliente": { "tipo": "registrado", "id": rosa }, "barbero_id": mateo, "servicio_ids": [corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] }),
    )
    .await;

    // ── Miércoles 21 de octubre: primera visita de Pablo, con Santiago al 40 %.
    mover_reloj(&app, "2026-10-21T16:00:00-05:00").await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let v = crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cliente": { "tipo": "registrado", "id": pablo }, "barbero_id": santiago, "servicio_ids": [corte],
                "propina": 2000, "pagos": [{ "metodo_pago_id": efectivo, "monto": 37000 }] }),
    )
    .await;
    assert_eq!(v["comision_barbero"], 14_000, "{v}");
    // Santiago pasa al 45 %: cada venta conserva el porcentaje con que se hizo (RN-15).
    let mut edicion = datos_santiago.clone();
    for campo in ["servicios", "horario", "visibilidad"] {
        edicion.as_object_mut().unwrap().remove(campo);
    }
    edicion["porcentaje_comision"] = json!(45);
    let r = pedir(
        &app,
        Method::PUT,
        &format!("/api/v1/barberos/{santiago}"),
        Some(&admin),
        Some(edicion),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);

    // ── Miércoles 11 de noviembre: un ocasional temprano y Pablo vuelve en la tarde.
    mover_reloj(&app, "2026-11-11T09:00:00-05:00").await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": mateo, "servicio_ids": [corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] }),
    )
    .await;
    mover_reloj(&app, "2026-11-11T15:00:00-05:00").await;
    let v = crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cliente": { "tipo": "registrado", "id": pablo }, "barbero_id": santiago, "servicio_ids": [corte, cejas],
                "propina": 5000, "pagos": [{ "metodo_pago_id": nequi, "monto": 50000 }] }),
    )
    .await;
    assert_eq!(
        (v["numero_ticket"].as_i64(), v["comision_barbero"].as_i64()),
        (Some(4), Some(20_250)),
        "{v}"
    );

    // ── Hoy, miércoles 18 de noviembre, 7:00 a. m.: la agenda de hoy y de mañana.
    mover_reloj(&app, "2026-11-18T07:00:00-05:00").await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let nuevo = |nombre: &str, celular: &str| json!({ "nombre": nombre, "celular": celular });
    let c_diana = agendar(
        &app,
        &admin,
        nuevo("Diana Ruiz", "3001234567"),
        corte,
        santiago,
        "2026-11-18T09:00:00-05:00",
    )
    .await;
    let gina = json!({ "nombre": "Gina Toro", "celular": "3006667777", "fecha_nacimiento": "1995-11-25" });
    agendar(&app, &admin, gina, corte, santiago, "2026-11-18T09:30:00-05:00").await;
    let c_carlos = agendar(
        &app,
        &admin,
        nuevo("Carlos Mora", "3002223333"),
        corte,
        mateo,
        "2026-11-18T09:00:00-05:00",
    )
    .await;
    let c_pablo = agendar(
        &app,
        &admin,
        json!(pablo),
        corte,
        mateo,
        "2026-11-18T10:00:00-05:00",
    )
    .await;
    let c_elena = agendar(
        &app,
        &admin,
        nuevo("Elena Gil", "3004445555"),
        corte,
        santiago,
        "2026-11-18T15:00:00-05:00",
    )
    .await;
    let c_fabio = agendar(
        &app,
        &admin,
        nuevo("Fabio Ríos", "3005556666"),
        corte,
        mateo,
        "2026-11-18T16:00:00-05:00",
    )
    .await;
    let c_hugo = agendar(
        &app,
        &admin,
        nuevo("Hugo Paz", "3007778888"),
        corte,
        mateo,
        "2026-11-19T10:00:00-05:00",
    )
    .await;
    let c_ivan = agendar(
        &app,
        &admin,
        nuevo("Iván Cruz", "3008889991"),
        corte,
        santiago,
        "2026-11-19T11:00:00-05:00",
    )
    .await;
    let c_juan = agendar(
        &app,
        &admin,
        nuevo("Juan Lara", "3009990001"),
        corte,
        daniel,
        "2026-11-19T12:00:00-05:00",
    )
    .await;
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{c_ivan}/confirmar"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "confirmada", "{}", r.json);
    // Mateo bloquea el mediodía; Daniel sale de vacaciones (su cita de mañana deja un aviso).
    crear(
        &app,
        &format!("/api/v1/barberos/{mateo}/bloqueos"),
        &admin,
        json!({ "fecha": "2026-11-18", "desde": "12:00", "hasta": "13:00", "motivo": "otro" }),
    )
    .await;
    crear(
        &app,
        &format!("/api/v1/barberos/{daniel}/ausencias"),
        &admin,
        json!({ "tipo": "vacaciones", "desde": "2026-11-18", "hasta": "2026-11-19", "observacion": "Viaje familiar." }),
    )
    .await;
    // Una solicitud de corrección de datos (Ley 1581).
    crear(
        &app,
        "/api/v1/clientes/solicitudes-titular",
        &admin,
        json!({ "cliente_id": rosa, "tipo": "actualizacion", "descripcion": "Pide corregir su correo." }),
    )
    .await;

    // 9:40: Diana, con 10 % de descuento y propina, paga en efectivo y por Nequi.
    mover_reloj(&app, "2026-11-18T09:40:00-05:00").await;
    let v = crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cita_id": c_diana, "servicio_ids": [corte, cejas], "descuento": { "descuento_id": diez, "motivo_descuento_id": motivo },
                "propina": 3000, "pagos": [{ "metodo_pago_id": efectivo, "monto": 20000 }, { "metodo_pago_id": nequi, "monto": 23500 }] }),
    )
    .await;
    assert_eq!(
        (v["venta_neta"].as_i64(), v["comision_barbero"].as_i64()),
        (Some(40_500), Some(18_225)),
        "{v}"
    );
    let diana = v["cliente"]["id"].as_i64().unwrap();
    // 9:50: una venta registrada por error y anulada no cuenta en ningún reporte.
    mover_reloj(&app, "2026-11-18T09:50:00-05:00").await;
    let v = crear(
        &app,
        "/api/v1/ventas",
        &admin,
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": mateo, "servicio_ids": [corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] }),
    )
    .await;
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/ventas/{}/anulacion", id(&v)),
        Some(&admin),
        Some(json!({ "motivo": "Se registró dos veces." })),
    )
    .await;
    assert!(r.estado.is_success(), "{}", r.json);
    mover_reloj(&app, "2026-11-18T10:20:00-05:00").await;
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{c_pablo}/iniciar"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "en_curso", "{}", r.json);
    mover_reloj(&app, "2026-11-18T10:30:00-05:00").await;
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{c_carlos}/inasistencia"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "no_asistio", "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{c_fabio}/cancelar"),
        Some(&admin),
        Some(json!({ "motivo": "solicitud_cliente", "avisar_cliente": false })),
    )
    .await;
    assert_eq!(r.json["estado"], "cancelada", "{}", r.json);

    // ── E2E-REP-01: el dashboard coincide con lo sembrado.
    let d = ver(&app, "/api/v1/reportes/dashboard", &admin).await;
    let hoy = json!({ "clientes_atendidos": 1, "servicios": 2, "subtotal": 45000, "descuentos": 4500, "ventas_netas": 40500,
                      "propinas": 3000, "total_cobrado": 43500, "comisiones": 18225, "ticket_promedio": 40500 });
    assert_eq!(
        (&d["fecha"], &d["hoy"], &d["totales_barberos"]),
        (&json!("2026-11-18"), &hoy, &hoy),
        "{d}"
    );
    // Frente al miércoles anterior hasta las 10:30: solo el ocasional de las 9:00 (35.000).
    assert_eq!(
        (
            d["comparacion"]["fecha"].as_str(),
            d["comparacion"]["totales"]["ventas_netas"].as_i64(),
            d["comparacion"]["variacion_ventas_netas"].as_f64(),
            d["comparacion"]["variacion_clientes"].as_f64()
        ),
        (Some("2026-11-11"), Some(35_000), Some(15.7), Some(0.0)),
        "{}",
        d["comparacion"]
    );
    assert_eq!(
        d["citas"],
        json!({ "atendidas": 1, "en_curso": 1, "proximas": 1, "sin_cerrar": 1, "no_asistio": 1, "canceladas": 1 })
    );
    let proxima = &d["proximas"][0];
    assert_eq!(
        (
            d["proximas"].as_array().unwrap().len(),
            proxima["cita_id"].as_i64(),
            proxima["hora"].as_str(),
            proxima["cliente"].as_str(),
            proxima["barbero"].as_str()
        ),
        (
            1,
            Some(c_elena),
            Some("3:00 p. m."),
            Some("Elena Gil"),
            Some("Santiago Ruiz")
        ),
        "{}",
        d["proximas"]
    );
    let tendencia: Vec<(String, i64)> = d["tendencia"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["fecha"].as_str().unwrap().to_owned(),
                t["ventas_netas"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(tendencia.len(), 12);
    assert_eq!(tendencia[0], ("2026-11-07".to_owned(), 0));
    assert_eq!(tendencia[4], ("2026-11-11".to_owned(), 80_000));
    assert_eq!(tendencia[11], ("2026-11-18".to_owned(), 40_500));
    assert_eq!(tendencia.iter().map(|t| t.1).sum::<i64>(), 120_500);
    let rendimiento = |barbero: i64| {
        let b = d["barberos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["barbero_id"] == barbero)
            .unwrap()
            .clone();
        (
            b["estado"].as_str().unwrap().to_owned(),
            b["clientes_atendidos"].as_i64().unwrap(),
            b["ventas_netas"].as_i64().unwrap(),
            b["propinas"].as_i64().unwrap(),
            b["minutos_ocupados"].as_u64().unwrap(),
            b["minutos_disponibles"].as_u64().unwrap(),
            b["ocupacion"].as_f64(),
        )
    };
    // Santiago: 3 citas de 30 min (atendida, sin cerrar y la de las 3:00) en 10 h.
    assert_eq!(
        rendimiento(santiago),
        ("activo".into(), 1, 40_500, 3_000, 90, 600, Some(15.0))
    );
    // Mateo: la cita en curso; la inasistencia y la cancelada no ocupan; el bloqueo descuenta 1 h.
    assert_eq!(rendimiento(mateo), ("activo".into(), 0, 0, 0, 30, 540, Some(5.6)));
    assert_eq!(rendimiento(daniel), ("vacaciones".into(), 0, 0, 0, 0, 0, None));
    assert_eq!(
        d["ausentes"],
        json!([{ "barbero_id": daniel, "nombre": "Daniel Ruiz", "tipo": "vacaciones", "hasta": "2026-11-19" }])
    );
    // Un día pasado se ve completo; sin ventas en la semana anterior no hay variación.
    let d = ver(&app, "/api/v1/reportes/dashboard?fecha=2026-11-11", &admin).await;
    assert_eq!(
        (
            d["hoy"]["clientes_atendidos"].as_i64(),
            d["hoy"]["ventas_netas"].as_i64(),
            d["comparacion"]["variacion_ventas_netas"].clone(),
            d["proximas"].as_array().unwrap().len()
        ),
        (Some(2), Some(80_000), Value::Null, 0),
        "{d}"
    );

    // ── RF-REP-02: resumen del mes en curso (1 al 18 de noviembre).
    let r = ver(&app, "/api/v1/reportes/resumen?mes=2026-11", &admin).await;
    assert_eq!(
        r["periodo"],
        json!({ "desde": "2026-11-01", "hasta": "2026-11-18", "texto": "1 al 18 de noviembre de 2026", "dias": 18 })
    );
    assert_eq!(
        r["totales"],
        json!({ "clientes_atendidos": 3, "servicios": 5, "subtotal": 125000, "descuentos": 4500, "ventas_netas": 120500,
                "propinas": 8000, "total_cobrado": 128500, "comisiones": 55975, "ticket_promedio": 40167 })
    );
    assert_eq!(
        r["por_semana"],
        json!([{ "desde": "2026-11-01", "hasta": "2026-11-01", "ventas_netas": 0 },
               { "desde": "2026-11-02", "hasta": "2026-11-08", "ventas_netas": 0 },
               { "desde": "2026-11-09", "hasta": "2026-11-15", "ventas_netas": 80000 },
               { "desde": "2026-11-16", "hasta": "2026-11-18", "ventas_netas": 40500 }])
    );
    assert_eq!(
        r["por_servicio"],
        json!([{ "servicio_id": corte, "nombre": "Corte clásico", "categoria": r["por_servicio"][0]["categoria"], "unidades": 3, "ingresos": 105000 },
               { "servicio_id": cejas, "nombre": "Diseño de cejas", "categoria": r["por_servicio"][1]["categoria"], "unidades": 2, "ingresos": 20000 }])
    );
    let metodos: Vec<(String, i64)> = r["por_metodo"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["metodo"].as_str().unwrap().to_owned(),
                m["total"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        metodos,
        vec![("nequi".into(), 73_500), ("efectivo".into(), 55_000)]
    );
    assert_eq!(
        r["por_barbero"],
        json!([{ "barbero_id": santiago, "nombre": "Santiago Ruiz", "clientes_atendidos": 2, "servicios": 4, "ventas_netas": 85500,
                 "comision": 38475, "propinas": 8000, "total_a_pagar": 46475 },
               { "barbero_id": mateo, "nombre": "Mateo Ruiz", "clientes_atendidos": 1, "servicios": 1, "ventas_netas": 35000,
                 "comision": 17500, "propinas": 0, "total_a_pagar": 17500 }])
    );
    // Sin periodo: el mes en curso hasta hoy (RF-REP-01).
    assert_eq!(ver(&app, "/api/v1/reportes/resumen", &admin).await, r);
    // Por rango: del 21 de octubre al 11 de noviembre.
    let r = ver(
        &app,
        "/api/v1/reportes/resumen?desde=2026-10-21&hasta=2026-11-11",
        &admin,
    )
    .await;
    assert_eq!(
        (
            r["periodo"]["texto"].as_str(),
            r["totales"]["clientes_atendidos"].as_i64(),
            r["totales"]["ventas_netas"].as_i64(),
            r["totales"]["comisiones"].as_i64()
        ),
        (
            Some("21 de octubre al 11 de noviembre de 2026"),
            Some(3),
            Some(115_000),
            Some(14_000 + 17_500 + 20_250)
        ),
        "{r}"
    );

    // ── E2E-REP-02: liquidación de Santiago: comisión = venta neta × su porcentaje en cada
    // venta, y las propinas incluyen la pagada por Nequi y la de efectivo (DA-09).
    let r = ver(
        &app,
        &format!("/api/v1/reportes/barberos/{santiago}?mes=2026-11"),
        &admin,
    )
    .await;
    assert_eq!(
        (
            r["barbero"]["nombre"].as_str(),
            r["barbero"]["porcentaje_comision"].as_f64(),
            r["barbero"]["tipo_vinculacion"].as_str(),
            r["clientes_distintos"].as_i64()
        ),
        (
            Some("Santiago Ruiz"),
            Some(45.0),
            Some("prestacion_servicios"),
            Some(2)
        ),
        "{}",
        r["barbero"]
    );
    assert_eq!(
        r["liquidacion"],
        json!({ "barbero_id": santiago, "nombre": "Santiago Ruiz", "clientes_atendidos": 2, "servicios": 4,
                "ventas_netas": 85500, "comision": 38475, "propinas": 8000, "total_a_pagar": 46475 })
    );
    assert_eq!(r["liquidacion"]["comision"], 85_500 * 45 / 100);
    let ventas: Vec<(i64, String, String, i64, i64, i64)> = r["ventas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["numero_ticket"].as_i64().unwrap(),
                v["cliente"].as_str().unwrap().to_owned(),
                v["servicios"].as_str().unwrap().to_owned(),
                v["ventas_netas"].as_i64().unwrap(),
                v["propina"].as_i64().unwrap(),
                v["comision"].as_i64().unwrap(),
            )
        })
        .collect();
    let ambos = "Corte clásico, Diseño de cejas".to_owned();
    assert_eq!(
        ventas,
        vec![
            (4, "Pablo Vélez".into(), ambos.clone(), 45_000, 5_000, 20_250),
            (5, "Diana Ruiz".into(), ambos, 40_500, 3_000, 18_225)
        ]
    );
    assert_eq!(
        r["servicios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (
                s["nombre"].as_str().unwrap().to_owned(),
                s["unidades"].as_i64().unwrap(),
                s["ingresos"].as_i64().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("Corte clásico".into(), 2, 70_000),
            ("Diseño de cejas".into(), 2, 20_000)
        ]
    );
    // Octubre: la venta conserva el 40 % que tenía Santiago entonces.
    let r = ver(
        &app,
        &format!("/api/v1/reportes/barberos/{santiago}?mes=2026-10"),
        &admin,
    )
    .await;
    assert_eq!(
        (
            r["liquidacion"]["comision"].as_i64(),
            r["liquidacion"]["propinas"].as_i64(),
            r["liquidacion"]["total_a_pagar"].as_i64()
        ),
        (Some(14_000), Some(2_000), Some(16_000)),
        "{r}"
    );

    // ── RF-REP-05: clientes del mes.
    let r = ver(&app, "/api/v1/reportes/clientes?mes=2026-11", &admin).await;
    assert_eq!(
        r["clientes"],
        json!({ "atendidos": 2, "ocasionales": 1, "nuevos": 1, "recurrentes": 1, "con_retorno": 1, "porcentaje_retorno": 50.0 })
    );
    assert_eq!(r["inasistencias"], 1);
    let destacados: Vec<(i64, i64, i64)> = r["mas_visitas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["cliente_id"].as_i64().unwrap(),
                c["visitas"].as_i64().unwrap(),
                c["ventas_netas"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(destacados, vec![(pablo, 1, 45_000), (diana, 1, 40_500)]);
    assert_eq!(
        (
            r["por_reactivar"]["dias_inactividad"].as_i64(),
            r["por_reactivar"]["total"].as_i64(),
            r["por_reactivar"]["clientes"].as_array().unwrap().len()
        ),
        (Some(60), Some(1), 1),
        "{}",
        r["por_reactivar"]
    );
    let inactiva = &r["por_reactivar"]["clientes"][0];
    assert_eq!(
        (
            inactiva["cliente_id"].as_i64(),
            inactiva["nombre"].as_str(),
            inactiva["ultima_visita"].as_str(),
            inactiva["dias_sin_volver"].as_i64(),
            inactiva["ultimo_servicio"].as_str()
        ),
        (
            Some(rosa),
            Some("Rosa Peña"),
            Some("2026-08-03"),
            Some(107),
            Some("Corte clásico")
        ),
        "{inactiva}"
    );
    assert!(
        inactiva["celular"].as_str().unwrap().ends_with("3001110001"),
        "{inactiva}"
    );
    let clientes_del_mes = r["clientes"].clone();

    // ── RF-DAS-05 a 07: tendencias.
    let r = ver(&app, "/api/v1/reportes/tendencias?mes=2026-11", &admin).await;
    assert_eq!(r["clientes"], clientes_del_mes);
    assert_eq!(r["por_servicio"][0]["unidades"], 3);
    let por_mes: Vec<(i64, i64)> = r["clientes_por_mes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["anio_actual"].as_i64().unwrap(),
                m["anio_anterior"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        por_mes,
        vec![
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (1, 0),
            (0, 0),
            (1, 0),
            (3, 0),
            (0, 0)
        ]
    );
    // Últimas 4 semanas: miércoles a las 9 (dos clientes) y a las 3 p. m. (uno), por semana.
    assert_eq!(
        r["horas_pico"],
        json!([{ "dia_semana": 3, "hora": 9, "promedio": 0.5 }, { "dia_semana": 3, "hora": 15, "promedio": 0.3 }])
    );

    // ── RF-DAS-08: alertas.
    let r = ver(&app, "/api/v1/reportes/alertas", &admin).await;
    let manana: Vec<(i64, String)> = r["sin_confirmar_manana"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["cita_id"].as_i64().unwrap(),
                c["barbero"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        manana,
        vec![(c_hugo, "Mateo Ruiz".into()), (c_juan, "Daniel Ruiz".into())],
        "la de Iván ya está confirmada"
    );
    let cumpleanos: Vec<(i64, String)> = r["cumpleanos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["cliente_id"].as_i64().unwrap(),
                c["fecha"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        cumpleanos,
        vec![(pablo, "2026-11-18".into()), (rosa, "2026-11-20".into())],
        "Gina cumple el 25: fuera de la semana"
    );
    assert_eq!(
        (
            r["ausentes"][0]["barbero_id"].as_i64(),
            r["por_reactivar"].as_i64(),
            r["solicitudes_titular"].as_i64(),
            r["avisos_no_leidos"].as_i64()
        ),
        (Some(daniel), Some(1), Some(1), Some(1)),
        "{r}"
    );

    // ── E2E-REP-03: el PDF del resumen (A4, encabezado con el periodo, pie con NIT y páginas).
    let (estado, cabeceras, pdf) =
        descargar(&app, "/api/v1/reportes/resumen.pdf?mes=2026-11", Some(&admin)).await;
    assert_eq!(estado, StatusCode::OK, "{}", String::from_utf8_lossy(&pdf));
    assert_eq!(cabeceras[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(
        cabeceras[header::CONTENT_DISPOSITION],
        "attachment; filename=\"resumen-2026-11.pdf\""
    );
    assert_eq!(cabeceras[header::CACHE_CONTROL], "private, no-store");
    assert!(pdf.starts_with(b"%PDF"));
    let enviado = String::from_utf8_lossy(&pdf);
    for esperado in [
        "filename=\"index.html\"",
        "filename=\"header.html\"",
        "filename=\"footer.html\"",
        "name=\"paperWidth\"",
        "8.27",
        "Diamante Barbería",
        "Resumen de ventas y servicios",
        "1 al 18 de noviembre de 2026",
        "Generado el 18/11/2026 a las 10:3",
        "NIT 900.123.456-7",
        "pageNumber",
        "totalPages",
        "$120.500",
        "Santiago Ruiz",
    ] {
        assert!(
            enviado.contains(esperado),
            "falta «{esperado}» en lo enviado a Gotenberg"
        );
    }
    let r = ver(&app, "/api/v1/auditoria?entidad=reporte", &admin).await;
    assert_eq!(r["total"], 1, "RF-SIS-07: la exportación queda auditada: {r}");
    assert_eq!(r["datos"][0]["accion"], "exportar_reporte");

    // RF-REP-04 y RF-REP-05 en PDF.
    let (estado, cabeceras, pdf) =
        descargar(&app, "/api/v1/reportes/barberos.pdf?mes=2026-11", Some(&admin)).await;
    assert_eq!(estado, StatusCode::OK);
    assert_eq!(
        cabeceras[header::CONTENT_DISPOSITION],
        "attachment; filename=\"barberos-2026-11.pdf\""
    );
    let enviado = String::from_utf8_lossy(&pdf);
    for esperado in [
        "Santiago Ruiz",
        "Mateo Ruiz",
        "Daniel Ruiz",
        "$46.475",
        "Reporte detallado de los barberos",
    ] {
        assert!(
            enviado.contains(esperado),
            "falta «{esperado}» en el PDF de los barberos"
        );
    }
    let (estado, _, pdf) = descargar(&app, "/api/v1/reportes/clientes.pdf?mes=2026-11", Some(&admin)).await;
    assert_eq!(estado, StatusCode::OK);
    assert!(String::from_utf8_lossy(&pdf).contains("Rosa Peña"));

    // Gotenberg caído: 503 que se puede reintentar, y nada queda auditado.
    fallar.store(true, Ordering::SeqCst);
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/reportes/resumen.pdf",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("pdf_no_disponible")),
        "{}",
        r.json
    );
    fallar.store(false, Ordering::SeqCst);
    assert_eq!(
        ver(&app, "/api/v1/auditoria?entidad=reporte", &admin).await["total"],
        3
    );

    // Periodos inválidos (RN-22) y parámetros desconocidos.
    for (ruta, campo) in [
        ("/api/v1/reportes/resumen?mes=2026-12", "mes"),
        ("/api/v1/reportes/resumen?mes=noviembre", "mes"),
        ("/api/v1/reportes/resumen?mes=2026-11&desde=2026-11-01", "mes"),
        (
            "/api/v1/reportes/resumen?desde=2026-11-10&hasta=2026-11-19",
            "hasta",
        ),
        ("/api/v1/reportes/resumen?desde=2026-11-10", "hasta"),
        (
            "/api/v1/reportes/resumen?desde=2026-11-12&hasta=2026-11-10",
            "desde",
        ),
        (
            "/api/v1/reportes/resumen?desde=2025-11-01&hasta=2026-11-18",
            "hasta",
        ),
        ("/api/v1/reportes/dashboard?fecha=2026-11-19", "fecha"),
    ] {
        let r = pedir(&app, Method::GET, ruta, Some(&admin), None).await;
        assert_eq!(
            (r.estado, r.json["errores"][0]["campo"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some(campo)),
            "{ruta}: {}",
            r.json
        );
    }
    for (ruta, estado) in [
        ("/api/v1/reportes/resumen?formato=xlsx", StatusCode::BAD_REQUEST),
        ("/api/v1/reportes/barberos/abc", StatusCode::BAD_REQUEST),
        ("/api/v1/reportes/barberos/999999", StatusCode::NOT_FOUND),
        ("/api/v1/reportes/barberos/999999.pdf", StatusCode::NOT_FOUND),
    ] {
        let r = pedir(&app, Method::GET, ruta, Some(&admin), None).await;
        assert_eq!(r.estado, estado, "{ruta}: {}", r.json);
    }

    // ── E2E-REP-04: Santiago, en «Mi reporte», solo ve su liquidación.
    let (_, barbero) = entrar(&app, "santiago@diamante.co").await;
    let r = ver(
        &app,
        &format!("/api/v1/reportes/barberos/{santiago}?mes=2026-11"),
        &barbero,
    )
    .await;
    assert_eq!(r["liquidacion"]["total_a_pagar"], 46_475);
    let (estado, cabeceras, _) = descargar(
        &app,
        &format!("/api/v1/reportes/barberos/{santiago}.pdf?mes=2026-11"),
        Some(&barbero),
    )
    .await;
    assert_eq!(
        (estado, cabeceras[header::CONTENT_DISPOSITION].to_str().unwrap()),
        (
            StatusCode::OK,
            format!("attachment; filename=\"barbero-{santiago}-2026-11.pdf\"").as_str()
        )
    );
    for ruta in [
        format!("/api/v1/reportes/barberos/{mateo}"),
        format!("/api/v1/reportes/barberos/{mateo}.pdf"),
    ] {
        let r = pedir(&app, Method::GET, &ruta, Some(&barbero), None).await;
        assert_eq!(
            r.estado,
            StatusCode::NOT_FOUND,
            "el de otro barbero «no existe»: {ruta}"
        );
    }
    let generales = [
        "/api/v1/reportes/dashboard",
        "/api/v1/reportes/tendencias",
        "/api/v1/reportes/alertas",
        "/api/v1/reportes/resumen",
        "/api/v1/reportes/resumen.pdf",
        "/api/v1/reportes/clientes",
        "/api/v1/reportes/clientes.pdf",
        "/api/v1/reportes/barberos.pdf",
    ];
    for ruta in generales {
        let r = pedir(&app, Method::GET, ruta, Some(&barbero), None).await;
        assert_eq!(r.estado, StatusCode::FORBIDDEN, "{ruta}: {}", r.json);
        let r = pedir(&app, Method::GET, ruta, None, None).await;
        assert_eq!(r.estado, StatusCode::UNAUTHORIZED, "{ruta} sin sesión");
    }
    assert_eq!(
        ver(&app, "/api/v1/auditoria?entidad=reporte", &admin).await["total"],
        4
    );
}

/// Sin Gotenberg configurado: los PDF responden 503, pero los permisos se revisan antes.
#[tokio::test]
async fn sin_gotenberg_los_pdf_responden_503_despues_de_los_permisos() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        _ => None,
    })
    .unwrap();
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(nucleo::RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);
    composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
        .await
        .unwrap();
    activar(&app, &base, "laura@diamante.co").await;
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let santiago = id(&crear(
        &app,
        "/api/v1/barberos",
        &admin,
        registro("Santiago", "1020304050", "santiago@diamante.co", vec![]),
    )
    .await);
    activar(&app, &base, "santiago@diamante.co").await;
    let (_, barbero) = entrar(&app, "santiago@diamante.co").await;

    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/reportes/resumen.pdf",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("pdf_no_disponible")),
        "{}",
        r.json
    );
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/reportes/resumen.pdf",
        Some(&barbero),
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::FORBIDDEN, "{}", r.json);
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/reportes/barberos/{santiago}.pdf"),
        Some(&barbero),
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::SERVICE_UNAVAILABLE, "{}", r.json);
    // La vista previa no depende de Gotenberg; sin ventas, todo en cero y sin ticket promedio.
    let r = ver(&app, "/api/v1/reportes/resumen", &admin).await;
    assert_eq!(
        (
            r["totales"]["clientes_atendidos"].as_i64(),
            r["totales"]["ticket_promedio"].clone()
        ),
        (Some(0), Value::Null)
    );
    let r = ver(&app, "/api/v1/reportes/dashboard", &admin).await;
    assert_eq!(r["barberos"][0]["barbero_id"], santiago);
    let r = ver(&app, "/api/v1/reportes/alertas", &admin).await;
    assert_eq!(r["por_reactivar"], 0);
    assert_eq!(
        ver(&app, "/api/v1/auditoria?entidad=reporte", &admin).await["total"],
        0
    );
}
