//! Entrega 2 (fase 8, requisitos *Should*) sobre PostgreSQL real con el enrutador completo:
//! segmentos configurables e indicadores de la base de clientes (RF-CFG-08, RF-CLI-04,
//! E2E-CLI-03).

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::pruebas_postgres::{CLAVE, SITIO, entrar, mover_reloj, pedir, registro, token_del_correo};
use crate::{enrutador, migraciones};

/// Ambiente con reloj de pruebas, la administradora con sesión y un barbero que hace cortes.
struct Ambiente {
    base: BaseDePrueba,
    composicion: Composicion,
    app: Router,
    admin: String,
    corte: i64,
    barbero: i64,
}

async fn ambiente(ahora: &str) -> Ambiente {
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
    mover_reloj(&app, ahora).await;
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
    let corte = crear(
        &app,
        &admin,
        "/servicios",
        json!({ "categoria_id": 1, "nombre": "Corte", "descripcion": "Corte.", "precio": 35_000, "duracion_min": 30 }),
    )
    .await;
    let barbero = crear(
        &app,
        &admin,
        "/barberos",
        registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![json!(corte)],
        ),
    )
    .await;
    Ambiente {
        base,
        composicion,
        app,
        admin,
        corte,
        barbero,
    }
}

async fn crear(app: &Router, sesion: &str, ruta: &str, cuerpo: Value) -> i64 {
    let r = pedir(
        app,
        Method::POST,
        &format!("/api/v1{ruta}"),
        Some(sesion),
        Some(cuerpo),
    )
    .await;
    assert!(r.estado.is_success(), "{ruta}: {} {}", r.estado, r.json);
    r.json["id"]
        .as_i64()
        .or_else(|| r.json["resumen"]["id"].as_i64())
        .unwrap()
}

impl Ambiente {
    /// Mueve el reloj y vuelve a entrar (un salto de días vence la sesión, como debe ser).
    async fn mover(&mut self, ahora: &str) {
        mover_reloj(&self.app, ahora).await;
        self.admin = entrar(&self.app, "laura@diamante.co").await.1;
    }

    async fn cliente(&self, nombre: &str, celular: &str) -> i64 {
        crear(
            &self.app,
            &self.admin,
            "/clientes",
            json!({ "nombre": nombre, "celular": celular }),
        )
        .await
    }

    /// Venta de un corte al cliente en la fecha del reloj: suma una visita.
    async fn vender(&self, cliente: i64, veces: usize) {
        let efectivo = pedir(
            &self.app,
            Method::GET,
            "/api/v1/ventas/catalogos",
            Some(&self.admin),
            None,
        )
        .await
        .json["metodos_pago"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["codigo"] == "efectivo")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        for _ in 0..veces {
            crear(
                &self.app,
                &self.admin,
                "/ventas",
                json!({ "cliente": { "tipo": "registrado", "id": cliente }, "barbero_id": self.barbero,
                        "servicio_ids": [self.corte], "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] }),
            )
            .await;
        }
    }

    async fn get(&self, ruta: &str) -> Value {
        let r = pedir(
            &self.app,
            Method::GET,
            &format!("/api/v1{ruta}"),
            Some(&self.admin),
            None,
        )
        .await;
        assert_eq!(r.estado, StatusCode::OK, "{ruta}: {}", r.json);
        r.json
    }
}

/// Segmento de cada cliente del listado, por nombre.
fn segmentos(listado: &Value) -> std::collections::BTreeMap<String, String> {
    listado["datos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["nombre"].as_str().unwrap().to_owned(),
                c["segmento"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// Los conteos de `meta` (SQL) son exactamente los segmentos que calcula el dominio para cada
/// cliente del listado.
fn meta_cuadra_con_el_listado(listado: &Value) {
    let por_segmento = &listado["meta"]["por_segmento"];
    let mut conteo = std::collections::BTreeMap::<String, i64>::new();
    for segmento in segmentos(listado).values() {
        *conteo.entry(segmento.clone()).or_default() += 1;
    }
    for segmento in ["sin_visitas", "nuevo", "recurrente", "frecuente", "inactivo"] {
        assert_eq!(
            por_segmento[segmento].as_i64().unwrap(),
            conteo.get(segmento).copied().unwrap_or(0),
            "{segmento}: {listado}"
        );
    }
    assert_eq!(listado["meta"]["registrados"], listado["total"]);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn segmentos_configurables_e_indicadores_de_la_base() {
    // Visitas en tres momentos: hace 90 días, hace exactamente 60 y hoy (18 de noviembre).
    let mut a = ambiente("2026-08-20T10:00:00-05:00").await;
    let diego = a.cliente("Diego Mora", "3001110001").await;
    a.vender(diego, 2).await;
    a.mover("2026-09-19T10:00:00-05:00").await;
    let gina = a.cliente("Gina Paz", "3001110002").await;
    a.vender(gina, 1).await;
    a.mover("2026-11-18T10:00:00-05:00").await;
    let ana = a.cliente("Ana Torres", "3001110003").await;
    a.vender(ana, 1).await;
    let bruno = a.cliente("Bruno Díaz", "3001110004").await;
    a.vender(bruno, 3).await;
    let carla = a.cliente("Carla Ríos", "3001110005").await;
    a.vender(carla, 6).await;
    a.cliente("Elena Gil", "3001110006").await;
    let felipe = a.cliente("Felipe Ruiz", "3001110007").await;
    a.vender(felipe, 1).await;
    let r = pedir(
        &a.app,
        Method::POST,
        &format!("/api/v1/clientes/{felipe}/anonimizacion"),
        Some(&a.admin),
        Some(json!({ "motivo": "Pidió borrar sus datos." })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT, "{}", r.json);

    // Umbrales por defecto (RN-20): Nuevo 1–2, Recurrente 3–5, Frecuente 6+, Sin volver +60 días.
    assert_eq!(
        a.get("/clientes/configuracion-segmentos").await,
        json!({ "max_visitas_nuevo": 2, "max_visitas_recurrente": 5, "dias_inactividad": 60 })
    );
    let listado = a.get("/clientes?por_pagina=100").await;
    assert_eq!(
        segmentos(&listado),
        [
            ("Ana Torres", "nuevo"),
            ("Bruno Díaz", "recurrente"),
            ("Carla Ríos", "frecuente"),
            ("Diego Mora", "inactivo"),
            ("Elena Gil", "sin_visitas"),
            // Su última visita fue hace exactamente 60 días: «sin volver» es *más* de 60.
            ("Gina Paz", "nuevo"),
        ]
        .into_iter()
        .map(|(n, s)| (n.to_owned(), s.to_owned()))
        .collect()
    );
    assert_eq!(
        listado["meta"],
        json!({ "registrados": 6, "dias_inactividad": 60,
                "por_segmento": { "sin_visitas": 1, "nuevo": 2, "recurrente": 1, "frecuente": 1, "inactivo": 1 } })
    );
    meta_cuadra_con_el_listado(&listado);
    // Los indicadores son de toda la base: no cambian con los filtros.
    let filtrado = a.get("/clientes?q=Ana&segmento=nuevo").await;
    assert_eq!(filtrado["total"], 1);
    assert_eq!(filtrado["meta"], listado["meta"]);

    // Validación (DA-07): Recurrente por encima de Nuevo, inactividad de 15 a 365 días.
    for (cuerpo, campos) in [
        (
            json!({ "max_visitas_nuevo": 3, "max_visitas_recurrente": 3, "dias_inactividad": 60 }),
            vec!["max_visitas_recurrente"],
        ),
        (
            json!({ "max_visitas_nuevo": 0, "max_visitas_recurrente": 5, "dias_inactividad": 10 }),
            vec!["max_visitas_nuevo", "dias_inactividad"],
        ),
        (
            json!({ "max_visitas_nuevo": 2, "max_visitas_recurrente": 101, "dias_inactividad": 366 }),
            vec!["max_visitas_recurrente", "dias_inactividad"],
        ),
    ] {
        let r = pedir(
            &a.app,
            Method::PUT,
            "/api/v1/clientes/configuracion-segmentos",
            Some(&a.admin),
            Some(cuerpo),
        )
        .await;
        assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
        let errores: Vec<&str> = r.json["errores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["campo"].as_str().unwrap())
            .collect();
        assert_eq!(errores, campos, "{}", r.json);
    }
    let r = pedir(
        &a.app,
        Method::PUT,
        "/api/v1/clientes/configuracion-segmentos",
        Some(&a.admin),
        Some(json!({ "max_visitas_nuevo": 2, "max_visitas_recurrente": 5 })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::BAD_REQUEST, "falta un campo: {}", r.json);

    // E2E-CLI-03: «Frecuente» desde 3 visitas y «Sin volver» tras 30 días.
    let r = pedir(
        &a.app,
        Method::PUT,
        "/api/v1/clientes/configuracion-segmentos",
        Some(&a.admin),
        Some(json!({ "max_visitas_nuevo": 1, "max_visitas_recurrente": 2, "dias_inactividad": 30 })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(
        r.json,
        json!({ "max_visitas_nuevo": 1, "max_visitas_recurrente": 2, "dias_inactividad": 30 })
    );
    let listado = a.get("/clientes?por_pagina=100").await;
    let segmento_de = |listado: &Value, nombre: &str| segmentos(listado)[nombre].clone();
    assert_eq!(segmento_de(&listado, "Bruno Díaz"), "frecuente");
    assert_eq!(segmento_de(&listado, "Gina Paz"), "inactivo");
    assert_eq!(segmento_de(&listado, "Ana Torres"), "nuevo");
    assert_eq!(
        listado["meta"]["por_segmento"],
        json!({ "sin_visitas": 1, "nuevo": 1, "recurrente": 0, "frecuente": 2, "inactivo": 2 })
    );
    assert_eq!(listado["meta"]["dias_inactividad"], 30);
    meta_cuadra_con_el_listado(&listado);
    // El filtro, la ficha y la búsqueda rápida usan los umbrales nuevos.
    assert_eq!(a.get("/clientes?segmento=frecuente").await["total"], 2);
    assert_eq!(
        a.get(&format!("/clientes/{bruno}")).await["segmento"],
        "frecuente"
    );
    assert_eq!(
        a.get("/clientes/buscar?q=Bruno").await[0]["segmento"],
        "frecuente"
    );
    // Y los reportes: «por reactivar» usa los mismos días (RF-SIS-06).
    let reporte = a.get("/reportes/clientes?mes=2026-11").await;
    assert_eq!(reporte["por_reactivar"]["dias_inactividad"], 30);
    assert_eq!(
        reporte["por_reactivar"]["total"],
        listado["meta"]["por_segmento"]["inactivo"]
    );

    // Más inactividad que la antigüedad de Diego (90 días): vuelve a contar por visitas.
    let r = pedir(
        &a.app,
        Method::PUT,
        "/api/v1/clientes/configuracion-segmentos",
        Some(&a.admin),
        Some(json!({ "max_visitas_nuevo": 1, "max_visitas_recurrente": 2, "dias_inactividad": 120 })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let listado = a.get("/clientes?por_pagina=100").await;
    assert_eq!(segmento_de(&listado, "Diego Mora"), "recurrente");
    assert_eq!(listado["meta"]["por_segmento"]["inactivo"], 0);
    meta_cuadra_con_el_listado(&listado);

    // Cada cambio queda en la auditoría con el antes y el después.
    let auditoria = a.get("/auditoria?entidad=configuracion_segmentos").await;
    let cambios = auditoria["datos"].as_array().unwrap();
    assert_eq!(cambios.len(), 2, "{auditoria}");
    assert!(cambios.iter().all(|c| c["accion"] == "cambiar_segmentos"));
    let primero = cambios
        .iter()
        .find(|c| c["despues"]["dias_inactividad"] == 30)
        .unwrap();
    assert_eq!(primero["antes"]["max_visitas_recurrente"], 5, "{primero}");

    // Un barbero no ve ni cambia los umbrales. (Su invitación de agosto ya venció: se le emite
    // un enlace nuevo, como con `diamante enlace-acceso`.)
    let enlace = a
        .composicion
        .identidad
        .enlace_de_acceso("santiago@diamante.co")
        .await
        .unwrap();
    let token = enlace.split_once("#token=").unwrap().1.to_owned();
    let r = pedir(
        &a.app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT, "{}", r.json);
    let (_, barbero) = entrar(&a.app, "santiago@diamante.co").await;
    for metodo in [Method::GET, Method::PUT] {
        let r = pedir(
            &a.app,
            metodo,
            "/api/v1/clientes/configuracion-segmentos",
            Some(&barbero),
            Some(json!({ "max_visitas_nuevo": 1, "max_visitas_recurrente": 2, "dias_inactividad": 30 })),
        )
        .await;
        assert_eq!(r.estado, StatusCode::FORBIDDEN, "{}", r.json);
    }
    a.base.eliminar().await;
}

/// Un día del resumen: (estado, tipo de día, disponibles, ocupados, libres, agenda llena).
fn dia(d: &Value) -> (String, String, u64, u64, Option<u64>, bool) {
    (
        d["estado"].as_str().unwrap().to_owned(),
        d["tipo_dia"].as_str().unwrap().to_owned(),
        d["minutos_disponibles"].as_u64().unwrap(),
        d["minutos_ocupados"].as_u64().unwrap(),
        d["minutos_libres"].as_u64(),
        d["agenda_llena"].as_bool().unwrap(),
    )
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn resumen_de_la_agenda_del_barbero() {
    // Miércoles 18 de noviembre, 8:00. Santiago trabaja de lunes a sábado de 9:00 a 19:00.
    let mut a = ambiente("2026-11-18T08:00:00-05:00").await;
    let token = token_del_correo(&a.base, "santiago@diamante.co").await;
    let r = pedir(
        &a.app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT, "{}", r.json);
    let b = a.barbero;
    let pablo = a.cliente("Pablo Vélez", "3001112233").await;
    let cita = |inicio: &str| json!({ "cliente_id": pablo, "servicio_id": a.corte, "barbero_id": b, "inicio": format!("2026-11-{inicio}:00-05:00") });
    // Jueves con cierre temprano a las 14:00 y sábado festivo (usa las franjas del domingo: no trabaja).
    crear(
        &a.app,
        &a.admin,
        "/negocio/dias-especiales",
        json!({ "fecha": "2026-11-19", "nombre": "Cierre temprano", "tipo": "cierre_temprano", "hora_cierre": "14:00:00" }),
    )
    .await;
    crear(
        &a.app,
        &a.admin,
        "/negocio/dias-especiales",
        json!({ "fecha": "2026-11-21", "nombre": "Festivo", "tipo": "festivo" }),
    )
    .await;
    // Hoy: cinco citas, un bloqueo de una hora.
    let mut hoy = Vec::new();
    for hora in ["09:00", "09:30", "11:00", "12:00", "15:00"] {
        hoy.push(crear(&a.app, &a.admin, "/agenda/citas", cita(&format!("18T{hora}"))).await);
    }
    crear(
        &a.app,
        &a.admin,
        &format!("/barberos/{b}/bloqueos"),
        json!({ "fecha": "2026-11-18", "desde": "16:00", "hasta": "17:00", "motivo": "otro" }),
    )
    .await;
    // Mañana: tres citas (una confirmada).
    let manana = crear(&a.app, &a.admin, "/agenda/citas", cita("19T09:00")).await;
    crear(&a.app, &a.admin, "/agenda/citas", cita("19T10:00")).await;
    crear(&a.app, &a.admin, "/agenda/citas", cita("19T11:00")).await;
    // Viernes: un bloqueo deja libres solo 20 minutos (18:40–19:00): no cabe un corte de 30.
    crear(
        &a.app,
        &a.admin,
        &format!("/barberos/{b}/bloqueos"),
        json!({ "fecha": "2026-11-20", "desde": "09:00", "hasta": "18:40", "motivo": "otro" }),
    )
    .await;
    // Lunes y martes de vacaciones.
    crear(
        &a.app,
        &a.admin,
        &format!("/barberos/{b}/ausencias"),
        json!({ "tipo": "vacaciones", "desde": "2026-11-23", "hasta": "2026-11-24", "observacion": "Viaje." }),
    )
    .await;

    // A las 10:00: la de las 9:00 se cobró, la de las 9:30 no llegó, la de las 11:00 se
    // confirmó, la de las 12:00 sigue sin confirmar y la de las 15:00 se canceló.
    a.mover("2026-11-18T10:00:00-05:00").await;
    let accion = |ruta: String, cuerpo: Option<Value>| {
        let (app, admin) = (a.app.clone(), a.admin.clone());
        async move {
            let r = pedir(
                &app,
                Method::POST,
                &format!("/api/v1{ruta}"),
                Some(&admin),
                cuerpo,
            )
            .await;
            assert!(r.estado.is_success(), "{ruta}: {} {}", r.estado, r.json);
        }
    };
    let efectivo = a.get("/ventas/catalogos").await["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    accion(
        "/ventas".into(),
        Some(json!({ "cita_id": hoy[0], "servicio_ids": [a.corte], "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] })),
    )
    .await;
    accion(format!("/agenda/citas/{}/inasistencia", hoy[1]), None).await;
    accion(format!("/agenda/citas/{}/confirmar", hoy[2]), None).await;
    accion(
        format!("/agenda/citas/{}/cancelar", hoy[4]),
        Some(json!({ "motivo": "solicitud_cliente", "avisar_cliente": false })),
    )
    .await;
    accion(format!("/agenda/citas/{manana}/confirmar"), None).await;

    let r = a.get(&format!("/agenda/resumen-barbero?barbero_id={b}")).await;
    assert_eq!(r["barbero_id"], b);
    assert_eq!(r["nombre"], "Santiago Ruiz");
    // Hoy: 4 citas (la cancelada aparte), 2 por atender. 9 h − 1 h de bloqueo = 540 minutos;
    // ocupan la atendida, la confirmada y la sin confirmar (90); la inasistencia ya no ocupa.
    assert_eq!(r["hoy"]["fecha"], "2026-11-18");
    assert_eq!(
        r["hoy"]["citas"],
        json!({ "total": 4, "sin_confirmar": 1, "confirmadas": 1, "en_curso": 0, "atendidas": 1,
                "no_asistio": 1, "canceladas": 1, "por_atender": 2 })
    );
    assert_eq!(
        dia(&r["hoy"]),
        ("trabaja".into(), "normal".into(), 540, 90, Some(450), false)
    );
    // Mañana: cierre temprano a las 14:00 (5 h), tres citas.
    assert_eq!(r["manana"]["fecha"], "2026-11-19");
    assert_eq!(
        dia(&r["manana"]),
        (
            "trabaja".into(),
            "cierre_temprano".into(),
            300,
            90,
            Some(210),
            false
        )
    );
    assert_eq!(r["manana"]["citas"]["confirmadas"], 1);
    assert_eq!(r["manana"]["citas"]["sin_confirmar"], 2);
    // Próximos 7 días: del jueves 19 al miércoles 25.
    let semana = &r["proximos_7_dias"];
    assert_eq!(
        (semana["desde"].as_str(), semana["hasta"].as_str()),
        (Some("2026-11-19"), Some("2026-11-25"))
    );
    assert_eq!(semana["dias"][0], r["manana"]);
    let dias: Vec<_> = semana["dias"].as_array().unwrap().iter().map(dia).collect();
    assert_eq!(
        dias,
        vec![
            (
                "trabaja".into(),
                "cierre_temprano".into(),
                300,
                90,
                Some(210),
                false
            ),
            // 20 minutos libres, pero sueltos: ningún corte cabe → agenda llena.
            ("trabaja".into(), "normal".into(), 20, 0, Some(20), true),
            ("no_trabaja".into(), "festivo".into(), 0, 0, None, false),
            ("no_trabaja".into(), "normal".into(), 0, 0, None, false),
            ("vacaciones".into(), "normal".into(), 0, 0, None, false),
            ("vacaciones".into(), "normal".into(), 0, 0, None, false),
            ("trabaja".into(), "normal".into(), 600, 0, Some(600), false),
        ]
    );
    assert_eq!(semana["dias_con_agenda_llena"], 1);
    assert_eq!(
        semana["citas"],
        json!({ "total": 3, "sin_confirmar": 2, "confirmadas": 1, "en_curso": 0, "atendidas": 0,
                "no_asistio": 0, "canceladas": 0, "por_atender": 3 })
    );

    // El barbero ve el suyo, igual, aunque pida el de otro; la administradora debe indicar cuál.
    let (_, barbero) = entrar(&a.app, "santiago@diamante.co").await;
    for ruta in [
        "/api/v1/agenda/resumen-barbero".to_owned(),
        "/api/v1/agenda/resumen-barbero?barbero_id=999".to_owned(),
    ] {
        let propio = pedir(&a.app, Method::GET, &ruta, Some(&barbero), None).await;
        assert_eq!(propio.estado, StatusCode::OK, "{}", propio.json);
        assert_eq!(propio.json, r);
    }
    for ruta in [
        "/api/v1/agenda/resumen-barbero",
        "/api/v1/agenda/resumen-barbero?barbero_id=999",
    ] {
        let sin = pedir(&a.app, Method::GET, ruta, Some(&a.admin), None).await;
        assert_eq!(sin.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", sin.json);
        assert_eq!(sin.json["errores"][0]["campo"], "barbero_id");
    }
    let r = pedir(&a.app, Method::GET, "/api/v1/agenda/resumen-barbero", None, None).await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);
    a.base.eliminar().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn vigilancia_de_la_operacion() {
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
    let operacion = || async { pedir(&app, Method::GET, "/api/salud/operacion", None, None).await };
    let estado = || async {
        let r = pedir(&app, Method::GET, "/api/v1/sistema/estado", Some(&admin), None).await;
        assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
        r.json
    };
    let sql = |consulta: &'static str| {
        let pool = base.pool.clone();
        async move {
            sqlx::query(consulta).execute(&pool).await.unwrap();
        }
    };

    // 1. La invitación espera en la cola y no hay SMTP: alerta. El monitor solo ve códigos.
    let r = operacion().await;
    assert_eq!(r.estado, StatusCode::SERVICE_UNAVAILABLE, "{}", r.json);
    assert_eq!(
        r.json,
        json!({ "estado": "alerta", "alertas": ["correo_sin_configurar"] })
    );
    let e = estado().await;
    assert_eq!(e["estado"], "alerta");
    assert_eq!(e["alertas"][0]["codigo"], "correo_sin_configurar");
    assert!(
        e["alertas"][0]["mensaje"]
            .as_str()
            .unwrap()
            .contains("no hay SMTP")
    );
    assert_eq!(e["cola"]["correo"]["configurado"], false);
    assert!(e["cola"]["correo"]["pendientes"].as_i64().unwrap() >= 1, "{e}");

    // 2. Los correos salieron: todo en orden.
    sql("UPDATE notificaciones.mensaje SET estado = 'enviado', enviado_en = now() WHERE canal = 'correo'")
        .await;
    let r = operacion().await;
    assert_eq!(
        (r.estado, r.json.clone()),
        (StatusCode::OK, json!({ "estado": "ok" }))
    );
    let e = estado().await;
    assert_eq!(
        (e["estado"].as_str(), e["alertas"].as_array().unwrap().len()),
        (Some("ok"), 0)
    );
    assert!(e["cola"]["correo"]["enviados_24h"].as_i64().unwrap() >= 1);

    // 3. El worker se detuvo: un trabajo vencido hace 20 minutos sin tomar…
    sql("INSERT INTO trabajos.trabajo (tipo, ejecutar_en) VALUES ('recordatorios', now() - interval '20 minutes')").await;
    assert_eq!(
        operacion().await.json,
        json!({ "estado": "alerta", "alertas": ["worker_detenido"] })
    );
    assert_eq!(estado().await["trabajos"]["vencidos"], 1);
    // …o ningún trabajo terminado en 30 minutos (los recordatorios corren cada 5).
    sql("DELETE FROM trabajos.trabajo").await;
    sql("INSERT INTO trabajos.trabajo (tipo, estado, terminado_en) VALUES ('recordatorios', 'completado', now() - interval '30 minutes')").await;
    assert_eq!(operacion().await.json["alertas"], json!(["worker_detenido"]));
    sql("INSERT INTO trabajos.trabajo (tipo, estado, terminado_en) VALUES ('recordatorios', 'completado', now())").await;
    assert_eq!(operacion().await.json, json!({ "estado": "ok" }));

    // 4. Errores del servidor: sin Gotenberg, cada PDF responde 503.
    for _ in 0..5 {
        let r = pedir(
            &app,
            Method::GET,
            "/api/v1/reportes/resumen.pdf",
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(r.estado, StatusCode::SERVICE_UNAVAILABLE);
    }
    assert_eq!(operacion().await.json["alertas"], json!(["errores_del_servidor"]));
    let e = estado().await;
    assert_eq!(e["api"]["ultimos_5_min"]["errores_del_servidor"], 5);
    assert!(
        e["api"]["desde_el_arranque"]["peticiones"].as_u64().unwrap() >= 10,
        "{e}"
    );
    let destacadas = e["api"]["rutas_destacadas"].as_array().unwrap();
    assert_eq!(destacadas[0]["ruta"], "GET /api/v1/reportes/resumen.pdf");
    assert_eq!(destacadas[0]["errores_del_servidor"], 5);
    assert!(
        destacadas[0]["latencia"].is_null(),
        "los PDF no cuentan para la latencia"
    );
    // Solo plantillas: ni identificadores ni consultas en las rutas.
    pedir(
        &app,
        Method::GET,
        "/api/v1/clientes/12345?x=1",
        Some(&admin),
        None,
    )
    .await;
    let e = estado().await;
    let rutas: Vec<&str> = e["api"]["rutas_destacadas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["ruta"].as_str().unwrap())
        .collect();
    assert!(
        rutas.iter().all(|r| !r.contains("12345") && !r.contains('?')),
        "{rutas:?}"
    );

    // 5. Solo la administradora ve el detalle.
    let r = pedir(&app, Method::GET, "/api/v1/sistema/estado", None, None).await;
    assert_eq!(r.estado, StatusCode::UNAUTHORIZED);
    base.eliminar().await;
}

/// Hallado en la búsqueda de errores de la fase 8: anular la venta de una cita la dejaba
/// «Atendida» para siempre y la cita no se podía volver a cobrar (la venta anulada ocupaba su
/// lugar único). Ahora vuelve a «Confirmada» y se cobra de nuevo; nunca con dos ventas vigentes.
#[tokio::test]
async fn anular_la_venta_de_una_cita_la_deja_por_cobrar() {
    let a = ambiente("2026-11-18T10:00:00-05:00").await;
    let pablo = a.cliente("Pablo Vélez", "3001112233").await;
    let cita = crear(
        &a.app,
        &a.admin,
        "/agenda/citas",
        json!({ "cliente_id": pablo, "servicio_id": a.corte, "barbero_id": a.barbero, "inicio": "2026-11-18T10:00:00-05:00" }),
    )
    .await;
    let efectivo = a.get("/ventas/catalogos").await["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let cobro = json!({ "cita_id": cita, "servicio_ids": [a.corte], "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] });
    let cobrar = || {
        let (app, admin, cobro) = (a.app.clone(), a.admin.clone(), cobro.clone());
        async move { pedir(&app, Method::POST, "/api/v1/ventas", Some(&admin), Some(cobro)).await }
    };
    let r = cobrar().await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let primera = r.json["id"].as_i64().unwrap();
    assert_eq!(
        a.get(&format!("/agenda/citas/{cita}")).await["estado"],
        "atendida"
    );
    let r = cobrar().await;
    assert_eq!(r.estado, StatusCode::CONFLICT, "ya cobrada: {}", r.json);

    // Se anula (p. ej., para corregir el servicio): la cita vuelve a quedar por cobrar.
    let r = pedir(
        &a.app,
        Method::POST,
        &format!("/api/v1/ventas/{primera}/anulacion"),
        Some(&a.admin),
        Some(json!({ "motivo": "Se cobró el servicio equivocado" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let detalle = a.get(&format!("/agenda/citas/{cita}")).await;
    assert_eq!(detalle["estado"], "confirmada", "{detalle}");
    let ultimo = detalle["historial"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(ultimo["evento"], "venta_anulada", "{detalle}");
    assert!(ultimo["detalle"].as_str().unwrap().contains(&primera.to_string()));
    assert_eq!(a.get(&format!("/clientes/{pablo}")).await["visitas"], 0);

    // Se vuelve a cobrar; y otra vez, no.
    let r = cobrar().await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    assert_eq!(
        a.get(&format!("/agenda/citas/{cita}")).await["estado"],
        "atendida"
    );
    assert_eq!(cobrar().await.estado, StatusCode::CONFLICT);
    assert_eq!(a.get(&format!("/clientes/{pablo}")).await["visitas"], 1);
    let caja = a.get("/ventas/caja").await;
    assert_eq!(
        (caja["numero_ventas"].as_i64(), caja["anuladas"].as_i64()),
        (Some(1), Some(1))
    );

    // Anular una venta sin cita no toca ninguna cita.
    let suelta = crear(
        &a.app,
        &a.admin,
        "/ventas",
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": a.barbero, "servicio_ids": [a.corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] }),
    )
    .await;
    let r = pedir(
        &a.app,
        Method::POST,
        &format!("/api/v1/ventas/{suelta}/anulacion"),
        Some(&a.admin),
        Some(json!({ "motivo": "Venta de prueba" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(
        a.get(&format!("/agenda/citas/{cita}")).await["estado"],
        "atendida"
    );
    a.base.eliminar().await;
}

/// Hallado en la búsqueda de errores de la fase 8: retirar a un barbero dejaba sus citas futuras
/// huérfanas (con recordatorios a los clientes) y sin ningún aviso a la administradora.
#[tokio::test]
async fn retirar_a_un_barbero_avisa_de_sus_citas_pendientes() {
    let a = ambiente("2026-11-18T10:00:00-05:00").await;
    let pablo = a.cliente("Pablo Vélez", "3001112233").await;
    let cita = |inicio: &str| json!({ "cliente_id": pablo, "servicio_id": a.corte, "barbero_id": a.barbero, "inicio": inicio });
    crear(
        &a.app,
        &a.admin,
        "/agenda/citas",
        cita("2026-11-18T15:00:00-05:00"),
    )
    .await;
    let viernes = crear(
        &a.app,
        &a.admin,
        "/agenda/citas",
        cita("2026-11-20T11:00:00-05:00"),
    )
    .await;
    let lunes = crear(
        &a.app,
        &a.admin,
        "/agenda/citas",
        cita("2026-11-23T12:00:00-05:00"),
    )
    .await;
    let avisos = |a: &Ambiente| {
        let (app, admin) = (a.app.clone(), a.admin.clone());
        async move {
            pedir(
                &app,
                Method::GET,
                "/api/v1/avisos?no_leidos=true",
                Some(&admin),
                None,
            )
            .await
            .json["datos"]
                .as_array()
                .unwrap()
                .clone()
        }
    };
    assert!(avisos(&a).await.is_empty());

    // Se retira desde el viernes: la cita de hoy sigue siendo suya; las del viernes y el lunes no.
    let r = pedir(
        &a.app,
        Method::POST,
        &format!("/api/v1/barberos/{}/retiro", a.barbero),
        Some(&a.admin),
        Some(json!({ "fecha": "2026-11-20", "motivo": "renuncia_voluntaria" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let avisos = avisos(&a).await;
    assert_eq!(avisos.len(), 1, "{avisos:?}");
    assert_eq!(avisos[0]["tipo"], "ausencia_con_citas");
    assert_eq!(
        avisos[0]["titulo"],
        "2 citas afectadas por el retiro de Santiago Ruiz"
    );
    assert_eq!(avisos[0]["datos"]["cita_ids"], json!([viernes, lunes]));
    assert!(avisos[0]["detalle"].as_str().unwrap().contains("Reasígnalas"));
    a.base.eliminar().await;
}

/// Carrera que abre el cambio anterior: anular la venta de una cita y volver a cobrarla a la vez
/// (dos veces). La anulación bloquea la venta y luego la cita; el cobro, la cita y luego espera
/// el índice único. Ningún `5xx` (ni bloqueo mutuo sin manejar), y siempre: «Atendida» si y solo
/// si la cita tiene exactamente una venta vigente.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::too_many_lines)]
async fn anular_y_volver_a_cobrar_a_la_vez() {
    let a = ambiente("2026-11-18T08:00:00-05:00").await;
    let efectivo = a.get("/ventas/catalogos").await["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let mut resultados = std::collections::BTreeMap::<String, usize>::new();
    for i in 0..20_i64 {
        let cliente = a
            .cliente(&format!("Cliente {i}"), &format!("30011100{i:02}"))
            .await;
        let minutos = 9 * 60 + i * 30;
        let inicio = format!("2026-11-18T{:02}:{:02}:00-05:00", minutos / 60, minutos % 60);
        let cita = crear(
            &a.app,
            &a.admin,
            "/agenda/citas",
            json!({ "cliente_id": cliente, "servicio_id": a.corte, "barbero_id": a.barbero, "inicio": inicio }),
        )
        .await;
        let cobro = json!({ "cita_id": cita, "servicio_ids": [a.corte], "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] });
        let venta = crear(&a.app, &a.admin, "/ventas", cobro.clone()).await;
        let mut tareas = tokio::task::JoinSet::new();
        for (ruta, cuerpo) in [
            (
                format!("/api/v1/ventas/{venta}/anulacion"),
                json!({ "motivo": "Se cobró el servicio equivocado" }),
            ),
            ("/api/v1/ventas".to_owned(), cobro.clone()),
            ("/api/v1/ventas".to_owned(), cobro.clone()),
        ] {
            let (app, admin) = (a.app.clone(), a.admin.clone());
            tareas.spawn(async move {
                let r = pedir(&app, Method::POST, &ruta, Some(&admin), Some(cuerpo)).await;
                (ruta.contains("anulacion"), r)
            });
        }
        let mut cobros_nuevos = 0;
        while let Some(resultado) = tareas.join_next().await {
            let (anulacion, r) = resultado.unwrap();
            assert!(!r.estado.is_server_error(), "vuelta {i}: {} {}", r.estado, r.json);
            if anulacion {
                assert_eq!(r.estado, StatusCode::OK, "la anulación siempre pasa: {}", r.json);
            } else if r.estado == StatusCode::CREATED {
                cobros_nuevos += 1;
            } else {
                assert_eq!(r.estado, StatusCode::CONFLICT, "{}", r.json);
            }
        }
        assert!(cobros_nuevos <= 1, "vuelta {i}: {cobros_nuevos} cobros nuevos");
        *resultados
            .entry(format!("{cobros_nuevos} cobro(s) nuevo(s)"))
            .or_default() += 1;
        let vigentes: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ventas.venta WHERE cita_id = $1 AND estado = 'registrada'",
        )
        .bind(cita)
        .fetch_one(&a.base.pool)
        .await
        .unwrap();
        let estado = a.get(&format!("/agenda/citas/{cita}")).await["estado"].clone();
        assert_eq!(vigentes, cobros_nuevos, "vuelta {i}");
        assert_eq!(
            estado == "atendida",
            vigentes == 1,
            "vuelta {i}: estado {estado} con {vigentes} ventas vigentes"
        );
    }
    assert_eq!(resultados.values().sum::<usize>(), 20, "{resultados:?}");
    a.base.eliminar().await;
}

/// Hallado en la búsqueda de errores de la fase 8: un monto inválido se convertía en `0` en
/// silencio (`unwrap_or(CERO)`): una propina negativa o enorme se registraba como 0 y un conteo
/// enorme del cierre de caja, también. Ahora cada uno es un `422` con su campo.
#[tokio::test]
async fn ningun_monto_invalido_se_vuelve_cero_en_silencio() {
    let a = ambiente("2026-11-18T10:00:00-05:00").await;
    let efectivo = a.get("/ventas/catalogos").await["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let venta = |propina: i64, monto: i64| {
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": a.barbero, "servicio_ids": [a.corte],
                "propina": propina, "pagos": [{ "metodo_pago_id": efectivo, "monto": monto }] })
    };
    for (cuerpo, campo) in [
        // Con la propina en 0 los pagos cuadrarían: antes, se registraba.
        (venta(-5_000, 35_000), "propina"),
        (venta(2_000_000_000_000, 35_000), "propina"),
        (venta(0, 2_000_000_000_000), "pagos"),
    ] {
        let r = pedir(
            &a.app,
            Method::POST,
            "/api/v1/ventas",
            Some(&a.admin),
            Some(cuerpo.clone()),
        )
        .await;
        assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{cuerpo}: {}", r.json);
        let campos: Vec<&str> = r.json["errores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["campo"].as_str().unwrap())
            .collect();
        assert!(campos.contains(&campo), "{cuerpo}: {}", r.json);
    }
    assert_eq!(
        a.get("/ventas/caja").await["numero_ventas"],
        0,
        "no quedó ninguna venta"
    );

    let r = pedir(
        &a.app,
        Method::POST,
        "/api/v1/ventas/cierres",
        Some(&a.admin),
        Some(json!({ "fecha": "2026-11-18", "contado": [{ "metodo_pago_id": efectivo, "monto": 2_000_000_000_000_i64 }] })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
    assert_eq!(r.json["errores"][0]["campo"], "contado", "{}", r.json);
    a.base.eliminar().await;
}

/// Hallado al revisar la vigilancia: cada método inventado (`RARO1`, `RARO2`…) creaba una clave
/// nueva en las métricas de rutas, y cualquiera, sin sesión, podía llenar la memoria de la API.
#[tokio::test]
async fn los_metodos_inventados_no_crean_claves_en_las_metricas() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_LIMITES_DE_USO" => Some("false".into()),
        _ => None,
    })
    .unwrap();
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(nucleo::RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);
    for i in 0..50 {
        let metodo = Method::from_bytes(format!("RARO{i}").as_bytes()).unwrap();
        let r = pedir(&app, metodo, "/api/v1/clientes", None, None).await;
        assert!(r.estado.is_client_error(), "{}", r.estado);
    }
    let rutas: Vec<String> = composicion
        .metricas
        .rutas_destacadas(1_000)
        .into_iter()
        .map(|r| r.ruta)
        .collect();
    assert_eq!(rutas, ["OTRO /api/v1/clientes"]);
    base.eliminar().await;
}
