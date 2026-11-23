//! Matriz de autorización de **todas** las rutas (OWASP ASVS V4, revisión de la fase 7), sobre
//! PostgreSQL real con el enrutador completo. El perfil de cada ruta es el de
//! `docs/diseno/api.md`:
//!
//! - Sin sesión: toda ruta que no es pública responde `401`.
//! - Barbero: toda ruta de la administradora responde `403` (o `404`: lo ajeno «no existe»),
//!   también las que escriben. Sus cuerpos solo tienen que deserializarse: el rol se revisa
//!   antes de buscar nada, así que nada se crea ni se cambia.
//! - Barbero con recursos de otro barbero (horario, servicios, bloqueos, citas, ventas,
//!   reportes): `403` o `404` (IDOR, ASVS V4.2.1); los listados solo traen lo suyo.
//! - Administradora: ninguna lectura le responde `401` ni `403`.
//!
//! Además, la tabla debe cubrir cada ruta que declara el código (`.route("…")` en
//! `modulos/*/src/http`): una ruta nueva sin clasificar hace fallar la prueba.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::pruebas_postgres::{
    CLAVE, SITIO, entrar, multipart, pedir, pedir_bytes, png, registro, token_del_correo,
};
use crate::{enrutador, migraciones};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perfil {
    /// Sin sesión (público, autenticación, webhooks firmados, descargas firmadas).
    Publico,
    /// Administradora y barberos.
    Personal,
    /// Solo la administradora.
    Admin,
    /// La administradora y el barbero dueño del recurso; la ruta apunta a **otro** barbero.
    AjenoAlBarbero,
    /// Solo barberos («Mi perfil»).
    Barbero,
}

#[derive(Debug, Clone)]
enum Cuerpo {
    Ninguno,
    Json(Value),
    /// Multipart con un PNG en ese campo.
    Archivo(&'static str),
}

#[derive(Debug, Clone)]
struct Caso {
    metodo: Method,
    /// Plantilla, como en el código: `/barberos/{id}/horario`.
    plantilla: &'static str,
    ruta: String,
    perfil: Perfil,
    cuerpo: Cuerpo,
}

fn caso(metodo: Method, plantilla: &'static str, ruta: String, perfil: Perfil, cuerpo: Cuerpo) -> Caso {
    Caso {
        metodo,
        plantilla,
        ruta,
        perfil,
        cuerpo,
    }
}

async fn ejecutar(app: &Router, c: &Caso, sesion: Option<&str>) -> (StatusCode, Value) {
    match &c.cuerpo {
        Cuerpo::Ninguno => {
            let r = pedir(app, c.metodo.clone(), &format!("/api/v1{}", c.ruta), sesion, None).await;
            (r.estado, r.json)
        }
        Cuerpo::Json(v) => {
            let r = pedir(
                app,
                c.metodo.clone(),
                &format!("/api/v1{}", c.ruta),
                sesion,
                Some(v.clone()),
            )
            .await;
            (r.estado, r.json)
        }
        Cuerpo::Archivo(campo) => {
            let imagen = png(64, 64);
            let (tipo, cuerpo) = multipart(&[(campo, Some("foto.png"), &imagen), ("tipo", None, b"rut")]);
            let (estado, _, bytes) = pedir_bytes(
                app,
                c.metodo.clone(),
                &format!("/api/v1{}", c.ruta),
                sesion,
                &tipo,
                cuerpo,
            )
            .await;
            (estado, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        }
    }
}

/// Rutas que declara el código bajo `/api/v1` (`.route("…"` en `modulos/*/src/http/*.rs` y en
/// las rutas propias de `app`, como la vigilancia; las absolutas, como `/api/salud/…`, no).
fn rutas_del_codigo() -> BTreeSet<String> {
    let manifiesto = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut archivos = vec![manifiesto.join("src/sistema.rs")];
    for modulo in std::fs::read_dir(manifiesto.join("../../modulos")).unwrap() {
        if let Ok(http) = std::fs::read_dir(modulo.unwrap().path().join("src/http")) {
            archivos.extend(http.map(|a| a.unwrap().path()));
        }
    }
    let mut rutas = BTreeSet::new();
    for archivo in archivos {
        let texto = std::fs::read_to_string(archivo).unwrap();
        for (i, _) in texto.match_indices(".route(") {
            let resto = texto[i + ".route(".len()..].trim_start();
            if let Some(sin_comilla) = resto.strip_prefix('"') {
                let ruta = &sin_comilla[..sin_comilla.find('"').unwrap()];
                if !ruta.starts_with("/_pruebas") && !ruta.starts_with("/api/") {
                    rutas.insert(ruta.to_owned());
                }
            }
        }
    }
    rutas
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn matriz_de_autorizacion_de_todas_las_rutas() {
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

    // Escenario: la administradora, Santiago (con sesión) y Mateo, con una cita, un bloqueo,
    // una ausencia y una venta de Mateo para intentar alcanzarlos desde la sesión de Santiago.
    composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, "laura@diamante.co", "Laura Gómez")
        .await
        .unwrap();
    let activar = |correo: &'static str| {
        let (app, base) = (app.clone(), &base);
        async move {
            let token = token_del_correo(base, correo).await;
            let r = pedir(
                &app,
                Method::POST,
                "/api/v1/auth/activacion",
                None,
                Some(json!({ "token": token, "contrasena": CLAVE })),
            )
            .await;
            assert_eq!(r.estado, StatusCode::NO_CONTENT, "{}", r.json);
        }
    };
    activar("laura@diamante.co").await;
    let (sesion_admin, admin) = entrar(&app, "laura@diamante.co").await;
    let admin_id = sesion_admin["usuario"]["id"].as_i64().unwrap();
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
    let mateo = crear(
        "/barberos",
        registro("Mateo", "1020304051", "mateo@diamante.co", vec![json!(corte)]),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    activar("santiago@diamante.co").await;
    let (_, barbero) = entrar(&app, "santiago@diamante.co").await;

    let hoy = chrono::Utc::now()
        .with_timezone(&nucleo::ZONA_HORARIA)
        .date_naive();
    let en = |dias: u64| hoy.checked_add_days(chrono::Days::new(dias)).unwrap();
    // Un día hábil (lunes a sábado) a más de una semana: cabe en el horario de los dos.
    let dia = (8..15)
        .map(en)
        .find(|d| chrono::Datelike::weekday(d) != chrono::Weekday::Sun)
        .unwrap();
    let a_las = |hora: &str| format!("{dia}T{hora}:00-05:00");
    let cita = |barbero_id: i64, hora: &str, nombre: &str, celular: &str| {
        let cuerpo = json!({ "cliente_nuevo": { "nombre": nombre, "celular": celular }, "servicio_id": corte,
                             "barbero_id": barbero_id, "inicio": a_las(hora) });
        crear("/agenda/citas", cuerpo)
    };
    let r = cita(mateo, "10:00", "Pablo Vélez", "3105559917").await;
    let (cita_mateo, cliente) = (r["id"].as_i64().unwrap(), r["cliente"]["id"].as_i64().unwrap());
    let cita_santiago = cita(santiago, "11:00", "Diana Ruiz", "3001234567").await["id"]
        .as_i64()
        .unwrap();
    let bloqueo_mateo = crear_en(
        &app,
        &admin,
        &format!("/barberos/{mateo}/bloqueos"),
        json!({ "fecha": dia, "desde": "15:00", "hasta": "16:00", "motivo": "otro" }),
    )
    .await;
    let ausencia_mateo = crear_en(
        &app,
        &admin,
        &format!("/barberos/{mateo}/ausencias"),
        json!({ "tipo": "vacaciones", "desde": en(40), "hasta": en(41), "observacion": "Viaje." }),
    )
    .await;
    let efectivo = pedir(&app, Method::GET, "/api/v1/ventas/catalogos", Some(&admin), None)
        .await
        .json["metodos_pago"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["codigo"] == "efectivo")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let venta_mateo = crear(
        "/ventas",
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": mateo, "servicio_ids": [corte],
                "pagos": [{ "metodo_pago_id": efectivo, "monto": 35000 }] }),
    )
    .await["id"]
        .as_i64()
        .unwrap();

    use Cuerpo::{Archivo, Json, Ninguno};
    use Method as M;
    use Perfil::{Admin, AjenoAlBarbero, Barbero, Personal, Publico};
    let franjas = json!({ "franjas": [{ "dia_semana": 1, "inicio": "09:00", "fin": "12:00" }] });
    let mut datos_barbero = registro("Mateo", "1020304051", "mateo@diamante.co", vec![]);
    for campo in ["servicios", "horario", "visibilidad"] {
        datos_barbero.as_object_mut().unwrap().remove(campo);
    }
    let servicio = json!({ "categoria_id": 1, "nombre": "Otro", "descripcion": "x", "precio": 1000, "duracion_min": 30 });
    let configuracion_agenda = json!({ "anticipacion_minima_min": 120, "ventana_reserva_dias": 30, "intervalo_min": 30,
        "aviso_cancelacion_min": 180, "recordatorio_24h": true, "recordatorio_2h": true,
        "politica_inasistencias": "sin_restriccion", "umbral_inasistencias": 3 });
    let casos = vec![
        // Público y autenticación.
        caso(
            M::GET,
            "/publico/landing",
            "/publico/landing".into(),
            Publico,
            Ninguno,
        ),
        caso(
            M::GET,
            "/publico/servicios",
            "/publico/servicios".into(),
            Publico,
            Ninguno,
        ),
        caso(
            M::GET,
            "/publico/barberos",
            format!("/publico/barberos?servicio_id={corte}"),
            Publico,
            Ninguno,
        ),
        caso(
            M::GET,
            "/publico/disponibilidad/calendario",
            format!("/publico/disponibilidad/calendario?servicio_id={corte}"),
            Publico,
            Ninguno,
        ),
        caso(
            M::GET,
            "/publico/disponibilidad/horas",
            format!("/publico/disponibilidad/horas?servicio_id={corte}&fecha={dia}"),
            Publico,
            Ninguno,
        ),
        caso(
            M::POST,
            "/publico/citas",
            "/publico/citas".into(),
            Publico,
            Json(json!({})),
        ),
        caso(
            M::GET,
            "/publico/citas/gestion/{token}",
            "/publico/citas/gestion/no-existe".into(),
            Publico,
            Ninguno,
        ),
        caso(
            M::POST,
            "/publico/citas/gestion/{token}/cancelar",
            "/publico/citas/gestion/no-existe/cancelar".into(),
            Publico,
            Ninguno,
        ),
        caso(
            M::POST,
            "/publico/citas/gestion/{token}/reprogramar",
            "/publico/citas/gestion/no-existe/reprogramar".into(),
            Publico,
            Json(json!({ "inicio": a_las("12:00"), "barbero_id": null })),
        ),
        caso(
            M::PUT,
            "/publico/citas/gestion/{token}/recordatorios",
            "/publico/citas/gestion/no-existe/recordatorios".into(),
            Publico,
            Json(json!({ "activos": false })),
        ),
        caso(
            M::POST,
            "/publico/verificaciones",
            "/publico/verificaciones".into(),
            Publico,
            Json(json!({ "celular": "1" })),
        ),
        caso(
            M::POST,
            "/publico/verificaciones/{id}/confirmar",
            "/publico/verificaciones/999/confirmar".into(),
            Publico,
            Json(json!({ "codigo": "0000" })),
        ),
        caso(
            M::POST,
            "/auth/sesion",
            "/auth/sesion".into(),
            Publico,
            Json(json!({ "correo": "x@y.co", "contrasena": "x" })),
        ),
        caso(M::GET, "/auth/sesion", "/auth/sesion".into(), Personal, Ninguno),
        caso(
            M::POST,
            "/auth/activacion",
            "/auth/activacion".into(),
            Publico,
            Json(json!({ "token": "x", "contrasena": CLAVE })),
        ),
        caso(
            M::POST,
            "/auth/recuperacion",
            "/auth/recuperacion".into(),
            Publico,
            Json(json!({ "correo": "x@y.co" })),
        ),
        caso(
            M::POST,
            "/auth/recuperacion/confirmar",
            "/auth/recuperacion/confirmar".into(),
            Publico,
            Json(json!({ "token": "x", "contrasena": CLAVE })),
        ),
        caso(
            M::PUT,
            "/auth/contrasena",
            "/auth/contrasena".into(),
            Personal,
            Json(json!({ "actual": "x", "nueva": "y" })),
        ),
        caso(
            M::GET,
            "/webhooks/whatsapp",
            "/webhooks/whatsapp".into(),
            Publico,
            Ninguno,
        ),
        caso(
            M::POST,
            "/webhooks/whatsapp",
            "/webhooks/whatsapp".into(),
            Publico,
            Json(json!({})),
        ),
        caso(
            M::GET,
            "/archivos/{id}",
            "/archivos/1?exp=1&firma=x".into(),
            Publico,
            Ninguno,
        ),
        // Identidad y negocio.
        caso(
            M::GET,
            "/administradores",
            "/administradores".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/administradores",
            "/administradores".into(),
            Admin,
            Json(json!({ "correo": "otra@diamante.co", "nombre": "Otra Admin" })),
        ),
        caso(
            M::PATCH,
            "/administradores/{id}",
            format!("/administradores/{admin_id}"),
            Admin,
            Json(json!({ "activo": false })),
        ),
        caso(M::GET, "/negocio", "/negocio".into(), Admin, Ninguno),
        caso(
            M::PUT,
            "/negocio",
            "/negocio".into(),
            Admin,
            Json(json!({ "nombre_comercial": "Hackeada", "porcentaje_comision_defecto": 99 })),
        ),
        caso(
            M::GET,
            "/negocio/horario-atencion",
            "/negocio/horario-atencion".into(),
            Personal,
            Ninguno,
        ),
        caso(
            M::GET,
            "/negocio/dias-especiales",
            "/negocio/dias-especiales".into(),
            Personal,
            Ninguno,
        ),
        caso(
            M::POST,
            "/negocio/dias-especiales",
            "/negocio/dias-especiales".into(),
            Admin,
            Json(json!({ "fecha": dia, "nombre": "Cierre", "tipo": "cerrado" })),
        ),
        caso(
            M::PUT,
            "/negocio/dias-especiales/{id}",
            "/negocio/dias-especiales/1".into(),
            Admin,
            Json(json!({ "fecha": dia, "nombre": "Cierre", "tipo": "cerrado" })),
        ),
        caso(
            M::DELETE,
            "/negocio/dias-especiales/{id}",
            "/negocio/dias-especiales/1".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/negocio/dias-especiales/festivos/{anio}",
            "/negocio/dias-especiales/festivos/2030".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/negocio/politicas",
            "/negocio/politicas".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/negocio/politicas",
            "/negocio/politicas".into(),
            Admin,
            Json(json!({ "tipo": "tratamiento_datos", "version": "9.9", "url": "https://x.co/p" })),
        ),
        caso(M::GET, "/auditoria", "/auditoria".into(), Admin, Ninguno),
        // Catálogo.
        caso(M::GET, "/categorias", "/categorias".into(), Personal, Ninguno),
        caso(M::GET, "/servicios", "/servicios".into(), Personal, Ninguno),
        caso(
            M::GET,
            "/servicios/{id}",
            format!("/servicios/{corte}"),
            Personal,
            Ninguno,
        ),
        caso(
            M::POST,
            "/servicios",
            "/servicios".into(),
            Admin,
            Json(servicio.clone()),
        ),
        caso(
            M::PUT,
            "/servicios/{id}",
            format!("/servicios/{corte}"),
            Admin,
            Json(servicio),
        ),
        caso(
            M::PATCH,
            "/servicios/{id}/estado",
            format!("/servicios/{corte}/estado"),
            Admin,
            Json(json!({ "activo": false })),
        ),
        caso(
            M::PUT,
            "/servicios/{id}/componentes",
            format!("/servicios/{corte}/componentes"),
            Admin,
            Json(json!({ "servicio_ids": [] })),
        ),
        // Barberos.
        caso(M::GET, "/yo", "/yo".into(), Barbero, Ninguno),
        caso(M::GET, "/barberos", "/barberos".into(), Admin, Ninguno),
        caso(
            M::POST,
            "/barberos",
            "/barberos".into(),
            Admin,
            Json(registro("Intruso", "1020304099", "intruso@diamante.co", vec![])),
        ),
        caso(
            M::PUT,
            "/barberos/orden",
            "/barberos/orden".into(),
            Admin,
            Json(json!({ "barbero_ids": [mateo, santiago] })),
        ),
        caso(
            M::GET,
            "/barberos/{id}",
            format!("/barberos/{mateo}"),
            Admin,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/barberos/{id}",
            format!("/barberos/{mateo}"),
            Admin,
            Json(datos_barbero),
        ),
        caso(
            M::PUT,
            "/barberos/{id}/visibilidad",
            format!("/barberos/{mateo}/visibilidad"),
            Admin,
            Json(
                json!({ "mostrar_en_landing": false, "recibe_citas_en_linea": false, "acceso_panel": false }),
            ),
        ),
        caso(
            M::POST,
            "/barberos/{id}/retiro",
            format!("/barberos/{mateo}/retiro"),
            Admin,
            Json(json!({ "fecha": hoy, "motivo": "renuncia_voluntaria" })),
        ),
        caso(
            M::GET,
            "/barberos/{id}/horario",
            format!("/barberos/{mateo}/horario"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/barberos/{id}/horario",
            format!("/barberos/{mateo}/horario"),
            AjenoAlBarbero,
            Json(franjas),
        ),
        caso(
            M::GET,
            "/barberos/{id}/servicios",
            format!("/barberos/{mateo}/servicios"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/barberos/{id}/servicios",
            format!("/barberos/{mateo}/servicios"),
            AjenoAlBarbero,
            Json(json!({ "servicio_ids": [] })),
        ),
        caso(
            M::GET,
            "/barberos/{id}/datos-privados",
            format!("/barberos/{mateo}/datos-privados"),
            Admin,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/barberos/{id}/datos-privados",
            format!("/barberos/{mateo}/datos-privados"),
            Admin,
            Json(json!({ "banco": "Otro" })),
        ),
        caso(
            M::POST,
            "/barberos/{id}/foto",
            format!("/barberos/{mateo}/foto"),
            Admin,
            Archivo("foto"),
        ),
        caso(
            M::GET,
            "/barberos/{id}/documentos",
            format!("/barberos/{mateo}/documentos"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/barberos/{id}/documentos",
            format!("/barberos/{mateo}/documentos"),
            Admin,
            Archivo("archivo"),
        ),
        caso(
            M::DELETE,
            "/barberos/{id}/documentos/{documento}",
            format!("/barberos/{mateo}/documentos/1"),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/barberos/{id}/documentos/{documento}/descarga",
            format!("/barberos/{mateo}/documentos/1/descarga"),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/barberos/{id}/ausencias",
            format!("/barberos/{mateo}/ausencias"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/barberos/{id}/ausencias",
            format!("/barberos/{mateo}/ausencias"),
            Admin,
            Json(json!({ "tipo": "vacaciones", "desde": en(50), "hasta": en(51), "observacion": "x" })),
        ),
        caso(
            M::POST,
            "/barberos/{id}/ausencias/{ausencia}/anular",
            format!("/barberos/{mateo}/ausencias/{ausencia_mateo}/anular"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/barberos/{id}/ausencias/{ausencia}/soporte",
            format!("/barberos/{mateo}/ausencias/{ausencia_mateo}/soporte"),
            Admin,
            Archivo("soporte"),
        ),
        caso(
            M::GET,
            "/barberos/{id}/bloqueos",
            format!("/barberos/{mateo}/bloqueos"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/barberos/{id}/bloqueos",
            format!("/barberos/{mateo}/bloqueos"),
            AjenoAlBarbero,
            Json(json!({ "fecha": dia, "desde": "17:00", "hasta": "18:00", "motivo": "otro" })),
        ),
        caso(
            M::DELETE,
            "/barberos/{id}/bloqueos/{bloqueo}",
            format!("/barberos/{mateo}/bloqueos/{bloqueo_mateo}"),
            AjenoAlBarbero,
            Ninguno,
        ),
        // Clientes.
        caso(M::GET, "/clientes", "/clientes".into(), Admin, Ninguno),
        caso(
            M::POST,
            "/clientes",
            "/clientes".into(),
            Personal,
            Json(json!({ "nombre": "x", "celular": "1" })),
        ),
        caso(
            M::GET,
            "/clientes/buscar",
            "/clientes/buscar?q=pablo".into(),
            Personal,
            Ninguno,
        ),
        caso(
            M::GET,
            "/clientes/configuracion-segmentos",
            "/clientes/configuracion-segmentos".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/clientes/configuracion-segmentos",
            "/clientes/configuracion-segmentos".into(),
            Admin,
            Json(json!({ "max_visitas_nuevo": 2, "max_visitas_recurrente": 5, "dias_inactividad": 60 })),
        ),
        caso(
            M::GET,
            "/clientes/{id}",
            format!("/clientes/{cliente}"),
            Admin,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/clientes/{id}",
            format!("/clientes/{cliente}"),
            Admin,
            Json(json!({ "nombre": "Cambiado", "celular": "3105559917" })),
        ),
        caso(
            M::POST,
            "/clientes/{id}/anonimizacion",
            format!("/clientes/{cliente}/anonimizacion"),
            Admin,
            Json(json!({ "motivo": "Intento del barbero" })),
        ),
        caso(
            M::GET,
            "/clientes/solicitudes-titular",
            "/clientes/solicitudes-titular".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/clientes/solicitudes-titular",
            "/clientes/solicitudes-titular".into(),
            Admin,
            Json(json!({ "cliente_id": cliente, "tipo": "consulta", "descripcion": "x" })),
        ),
        caso(
            M::PATCH,
            "/clientes/solicitudes-titular/{id}",
            "/clientes/solicitudes-titular/1".into(),
            Admin,
            Json(json!({ "respuesta": "x" })),
        ),
        // Agenda.
        caso(
            M::GET,
            "/agenda/tablero",
            "/agenda/tablero".into(),
            Admin,
            Ninguno,
        ),
        caso(M::GET, "/agenda/citas", "/agenda/citas".into(), Personal, Ninguno),
        caso(
            M::POST,
            "/agenda/citas",
            "/agenda/citas".into(),
            Admin,
            Json(
                json!({ "cliente_id": cliente, "servicio_id": corte, "barbero_id": santiago, "inicio": a_las("14:00") }),
            ),
        ),
        caso(
            M::GET,
            "/agenda/citas/{id}",
            format!("/agenda/citas/{cita_mateo}"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/reprogramar",
            format!("/agenda/citas/{cita_mateo}/reprogramar"),
            Admin,
            Json(json!({ "inicio": a_las("16:30"), "barbero_id": null })),
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/cancelar",
            format!("/agenda/citas/{cita_mateo}/cancelar"),
            Admin,
            Json(json!({ "motivo": "otro" })),
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/confirmar",
            format!("/agenda/citas/{cita_mateo}/confirmar"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/iniciar",
            format!("/agenda/citas/{cita_mateo}/iniciar"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/inasistencia",
            format!("/agenda/citas/{cita_mateo}/inasistencia"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/citas/{id}/recordatorio",
            format!("/agenda/citas/{cita_mateo}/recordatorio"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/citas/reasignacion",
            "/agenda/citas/reasignacion".into(),
            Admin,
            Json(json!({ "cita_ids": [cita_mateo], "barbero_destino_id": santiago })),
        ),
        caso(
            M::POST,
            "/agenda/citas/aviso-reprogramacion",
            "/agenda/citas/aviso-reprogramacion".into(),
            Admin,
            Json(json!({ "cita_ids": [cita_mateo] })),
        ),
        caso(
            M::GET,
            "/agenda/disponibilidad/horas",
            format!("/agenda/disponibilidad/horas?servicio_id={corte}&barbero_id={mateo}&fecha={dia}"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/agenda/conflictos",
            "/agenda/conflictos".into(),
            AjenoAlBarbero,
            Json(json!({ "barbero_id": mateo, "ausencia": { "desde": dia, "hasta": dia } })),
        ),
        caso(
            M::GET,
            "/agenda/pendientes-de-cierre",
            "/agenda/pendientes-de-cierre".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/agenda/configuracion",
            "/agenda/configuracion".into(),
            Admin,
            Ninguno,
        ),
        // El barbero siempre recibe el suyo (aunque pida otro): como «Lo que llevas hoy».
        caso(
            M::GET,
            "/agenda/resumen-barbero",
            format!("/agenda/resumen-barbero?barbero_id={mateo}"),
            Personal,
            Ninguno,
        ),
        caso(
            M::PUT,
            "/agenda/configuracion",
            "/agenda/configuracion".into(),
            Admin,
            Json(configuracion_agenda),
        ),
        // Vigilancia (RNF-OBS-02).
        caso(
            M::GET,
            "/sistema/estado",
            "/sistema/estado".into(),
            Admin,
            Ninguno,
        ),
        // Notificaciones.
        caso(M::GET, "/avisos", "/avisos".into(), Admin, Ninguno),
        caso(
            M::POST,
            "/avisos/{id}/leido",
            "/avisos/1/leido".into(),
            Admin,
            Ninguno,
        ),
        caso(M::GET, "/mensajes", "/mensajes".into(), Admin, Ninguno),
        // Ventas.
        caso(M::GET, "/ventas", "/ventas".into(), Personal, Ninguno),
        caso(
            M::POST,
            "/ventas",
            "/ventas".into(),
            Personal,
            Json(json!({ "servicio_ids": [] })),
        ),
        caso(
            M::GET,
            "/ventas/catalogos",
            "/ventas/catalogos".into(),
            Personal,
            Ninguno,
        ),
        caso(M::GET, "/ventas/caja", "/ventas/caja".into(), Admin, Ninguno),
        caso(
            M::GET,
            "/ventas/resumen-barbero",
            "/ventas/resumen-barbero".into(),
            Personal,
            Ninguno,
        ),
        caso(
            M::GET,
            "/ventas/cierres",
            format!("/ventas/cierres?fecha={hoy}"),
            Admin,
            Ninguno,
        ),
        caso(
            M::POST,
            "/ventas/cierres",
            "/ventas/cierres".into(),
            Admin,
            Json(json!({ "fecha": hoy, "contado": [] })),
        ),
        caso(
            M::GET,
            "/ventas/metodos-pago",
            "/ventas/metodos-pago".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::PATCH,
            "/ventas/metodos-pago/{id}",
            format!("/ventas/metodos-pago/{efectivo}"),
            Admin,
            Json(json!({ "activo": false })),
        ),
        caso(
            M::GET,
            "/ventas/{id}",
            format!("/ventas/{venta_mateo}"),
            AjenoAlBarbero,
            Ninguno,
        ),
        caso(
            M::POST,
            "/ventas/{id}/anulacion",
            format!("/ventas/{venta_mateo}/anulacion"),
            Admin,
            Json(json!({ "motivo": "Intento del barbero" })),
        ),
        // Reportes.
        caso(
            M::GET,
            "/reportes/dashboard",
            "/reportes/dashboard".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/tendencias",
            "/reportes/tendencias".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/alertas",
            "/reportes/alertas".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/resumen",
            "/reportes/resumen".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/resumen.pdf",
            "/reportes/resumen.pdf".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/clientes",
            "/reportes/clientes".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/clientes.pdf",
            "/reportes/clientes.pdf".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/barberos.pdf",
            "/reportes/barberos.pdf".into(),
            Admin,
            Ninguno,
        ),
        caso(
            M::GET,
            "/reportes/barberos/{archivo}",
            format!("/reportes/barberos/{mateo}"),
            AjenoAlBarbero,
            Ninguno,
        ),
    ];

    // La tabla cubre cada ruta del código (salvo `/media/{*ruta}`, que se prueba aparte).
    let en_tabla: BTreeSet<String> = casos.iter().map(|c| c.plantilla.to_owned()).collect();
    let faltan: Vec<_> = rutas_del_codigo()
        .into_iter()
        .filter(|r| r != "/media/{*ruta}" && !en_tabla.contains(r))
        .collect();
    assert!(faltan.is_empty(), "rutas sin clasificar en la matriz: {faltan:?}");

    let mut fallas = Vec::new();
    let denegado = |estado: StatusCode| matches!(estado, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND);
    for c in &casos {
        // 1. Sin sesión.
        let (estado, cuerpo) = ejecutar(&app, c, None).await;
        match c.perfil {
            Publico => {
                if estado == StatusCode::UNAUTHORIZED && cuerpo["codigo"] == "no_autenticado" {
                    fallas.push(format!("{} {} pide sesión siendo pública", c.metodo, c.plantilla));
                }
            }
            _ => {
                if (estado, cuerpo["codigo"].as_str()) != (StatusCode::UNAUTHORIZED, Some("no_autenticado")) {
                    fallas.push(format!(
                        "{} {} sin sesión → {estado} {cuerpo}",
                        c.metodo, c.plantilla
                    ));
                }
            }
        }
        // 2. Barbero (Santiago).
        let (estado, cuerpo) = ejecutar(&app, c, Some(&barbero)).await;
        match c.perfil {
            Admin | AjenoAlBarbero if !denegado(estado) => {
                fallas.push(format!(
                    "{} {} con el barbero → {estado} {cuerpo}",
                    c.metodo, c.plantilla
                ));
            }
            Personal | Barbero if matches!(estado, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
                fallas.push(format!(
                    "{} {} con el barbero → {estado} {cuerpo}",
                    c.metodo, c.plantilla
                ));
            }
            _ => {}
        }
        // 3. Administradora (solo lecturas: las escrituras se prueban en sus módulos).
        if c.metodo == M::GET && c.perfil != Publico && c.perfil != Barbero {
            let (estado, cuerpo) = ejecutar(&app, c, Some(&admin)).await;
            // Sin Gotenberg en esta prueba, los PDF responden 503 (no es un asunto de permisos).
            let sin_pdf = cuerpo["codigo"] == "pdf_no_disponible";
            if matches!(estado, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
                || (estado.is_server_error() && !sin_pdf)
            {
                fallas.push(format!(
                    "{} {} con la administradora → {estado} {cuerpo}",
                    c.metodo, c.plantilla
                ));
            }
        }
    }
    assert!(
        fallas.is_empty(),
        "fallas de autorización:\n{}",
        fallas.join("\n")
    );

    // Nada de lo que intentó el barbero cambió los datos de la administradora ni de Mateo.
    let r = pedir(&app, Method::GET, "/api/v1/negocio", Some(&admin), None).await;
    assert_ne!(r.json["nombre_comercial"], "Hackeada");
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/citas/{cita_mateo}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "sin_confirmar", "{}", r.json);
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/ventas/{venta_mateo}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "registrada", "{}", r.json);
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/barberos/{mateo}/bloqueos"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json.as_array().map(Vec::len), Some(1), "{}", r.json);
    let r = pedir(&app, Method::GET, "/api/v1/administradores", Some(&admin), None).await;
    assert!(r.json.to_string().contains("laura@diamante.co"), "{}", r.json);

    // Los listados del barbero solo traen lo suyo, aunque pida los de otro (RN-26).
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/citas?barbero_id={mateo}&desde={dia}&hasta={dia}"),
        Some(&barbero),
        None,
    )
    .await;
    let ids: Vec<i64> = r.json.as_array().map_or_else(Vec::new, |citas| {
        citas.iter().filter_map(|c| c["id"].as_i64()).collect()
    });
    assert!(
        !ids.contains(&cita_mateo),
        "el barbero vio una cita de otro: {}",
        r.json
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/citas?desde={dia}&hasta={dia}"),
        Some(&barbero),
        None,
    )
    .await;
    assert!(
        r.json.to_string().contains(&format!("\"id\":{cita_santiago}")),
        "{}",
        r.json
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/ventas?barbero_id={mateo}"),
        Some(&barbero),
        None,
    )
    .await;
    assert!(
        !r.json.to_string().contains(&format!("\"id\":{venta_mateo}")),
        "el barbero vio una venta de otro: {}",
        r.json
    );
}

async fn crear_en(app: &Router, sesion: &str, ruta: &str, cuerpo: Value) -> i64 {
    let r = pedir(
        app,
        Method::POST,
        &format!("/api/v1{ruta}"),
        Some(sesion),
        Some(cuerpo),
    )
    .await;
    assert!(r.estado.is_success(), "{ruta}: {}", r.json);
    r.json["id"].as_i64().unwrap()
}
