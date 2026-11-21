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

// ---------------------------------------------------------------------------------------------
// Fase 2: catálogo, equipo y archivos
// ---------------------------------------------------------------------------------------------

/// Petición con un cuerpo arbitrario (multipart) y respuesta en bytes.
async fn pedir_bytes(
    app: &Router,
    metodo: Method,
    ruta: &str,
    sesion: Option<&str>,
    tipo: &str,
    cuerpo: Vec<u8>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut peticion = Request::builder()
        .method(metodo)
        .uri(ruta)
        .header(header::ORIGIN, SITIO);
    if let Some(token) = sesion {
        peticion = peticion.header(header::COOKIE, format!("__Host-sesion={token}"));
    }
    if !tipo.is_empty() {
        peticion = peticion.header(header::CONTENT_TYPE, tipo);
    }
    let respuesta = app
        .clone()
        .oneshot(peticion.body(Body::from(cuerpo)).unwrap())
        .await
        .unwrap();
    let (partes, cuerpo) = respuesta.into_parts();
    (
        partes.status,
        partes.headers,
        cuerpo.collect().await.unwrap().to_bytes().to_vec(),
    )
}

/// Cuerpo `multipart/form-data`: (campo, nombre de archivo si es archivo, contenido).
fn multipart(partes: &[(&str, Option<&str>, &[u8])]) -> (String, Vec<u8>) {
    let frontera = "frontera-de-prueba-7MA4YWxkTrZu0gW";
    let mut cuerpo = Vec::new();
    for (campo, archivo, contenido) in partes {
        cuerpo.extend_from_slice(format!("--{frontera}\r\n").as_bytes());
        let cabecera = match archivo {
            Some(nombre) => format!(
                "Content-Disposition: form-data; name=\"{campo}\"; filename=\"{nombre}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            ),
            None => format!("Content-Disposition: form-data; name=\"{campo}\"\r\n\r\n"),
        };
        cuerpo.extend_from_slice(cabecera.as_bytes());
        cuerpo.extend_from_slice(contenido);
        cuerpo.extend_from_slice(b"\r\n");
    }
    cuerpo.extend_from_slice(format!("--{frontera}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={frontera}"), cuerpo)
}

/// Cabecera PNG mínima con las medidas indicadas.
fn png(ancho: u32, alto: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend_from_slice(&ancho.to_be_bytes());
    bytes.extend_from_slice(&alto.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// Token del último enlace enviado a `correo`, leído de la cola de salida.
async fn token_del_correo(base: &BaseDePrueba, correo: &str) -> String {
    let variables: Value = sqlx::query_scalar(
        "SELECT variables FROM notificaciones.mensaje WHERE destinatario = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(correo)
    .fetch_one(&base.pool)
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

fn registro(nombres: &str, documento: &str, correo: &str, servicios: Vec<Value>) -> Value {
    json!({
        "tipo_documento": "CC", "numero_documento": documento, "nombres": nombres, "apellidos": "Ruiz",
        "fecha_nacimiento": "1995-03-02", "celular": "3105559917", "correo": correo, "especialidad": "Fade",
        "anios_experiencia": 6, "instagram": "@santi.cuts", "fecha_ingreso": "2025-01-15",
        "tipo_vinculacion": "prestacion_servicios",
        "servicios": servicios,
        "horario": (1..=6).map(|dia| json!({ "dia_semana": dia, "inicio": "09:00", "fin": "19:00" })).collect::<Vec<_>>(),
        "visibilidad": { "mostrar_en_landing": true, "recibe_citas_en_linea": true, "acceso_panel": true }
    })
}

#[tokio::test]
async fn equipo_catalogo_y_archivos_de_punta_a_punta() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let raiz = std::env::temp_dir().join(format!("diamante-app-{}", std::process::id()));
    let raiz_texto = raiz.display().to_string();
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_ARCHIVOS_DIR" => Some(raiz_texto.clone()),
        _ => None,
    })
    .unwrap();
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);
    let fecha = |dias: i32| {
        let pool = base.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT ((now() AT TIME ZONE 'America/Bogota')::date + $1)::text")
                .bind(dias)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

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

    // Catálogo: dos servicios y un combo.
    let servicio = |nombre: &str, precio: i64| json!({ "categoria_id": 1, "nombre": nombre, "descripcion": "Servicio.", "precio": precio, "duracion_min": 30 });
    let corte = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        Some(&admin),
        Some(servicio("Corte clásico", 35_000)),
    )
    .await
    .json;
    let cejas = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        Some(&admin),
        Some(servicio("Diseño de cejas", 10_000)),
    )
    .await
    .json;
    let combo = json!({ "tipo": "combo", "categoria_id": 3, "nombre": "Corte + cejas", "descripcion": "Combo.", "precio": 40_000,
                        "duracion_min": 60, "componentes": [corte["id"], cejas["id"]] });
    let combo = pedir(&app, Method::POST, "/api/v1/servicios", Some(&admin), Some(combo)).await;
    assert_eq!(combo.estado, StatusCode::CREATED, "{}", combo.json);

    // Registro del barbero: cuenta, invitación, horario y servicios (E2E-BAR-01, E2E-SER-01).
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/barberos",
        Some(&admin),
        Some(registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![corte["id"].clone(), combo.json["id"].clone()],
        )),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let santiago = r.json["id"].as_i64().unwrap();
    assert_eq!(
        r.json["porcentaje_comision"], 50.0,
        "toma la comisión por defecto del negocio"
    );
    assert_eq!(r.json["estado"]["codigo"], "activo");
    assert_eq!(r.json["horario"]["total_horas_semana"], 60.0);
    assert_eq!(r.json["cuenta"]["pendiente_activacion"], true);
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/barberos",
        Some(&admin),
        Some(registro("Otro", "1020304050", "otro@diamante.co", vec![])),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("documento_en_uso"))
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/servicios/{}", combo.json["id"]),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (
            r.json["barberos_que_lo_ofrecen"].as_i64(),
            r.json["advertencia"].is_null()
        ),
        (Some(1), true)
    );

    // El barbero activa su cuenta con el correo y entra a su panel.
    let token = token_del_correo(&base, "santiago@diamante.co").await;
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/auth/activacion",
        None,
        Some(json!({ "token": token, "contrasena": CLAVE })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    let (sesion, barbero) = entrar(&app, "santiago@diamante.co").await;
    assert_eq!(sesion["panel"], "/barbero");
    let yo = pedir(&app, Method::GET, "/api/v1/yo", Some(&barbero), None).await;
    assert_eq!(
        (yo.estado, yo.json["id"].as_i64()),
        (StatusCode::OK, Some(santiago))
    );
    assert_eq!(
        pedir(&app, Method::GET, "/api/v1/barberos", Some(&barbero), None)
            .await
            .estado,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        pedir(
            &app,
            Method::GET,
            "/api/v1/barberos/999/horario",
            Some(&barbero),
            None
        )
        .await
        .estado,
        StatusCode::NOT_FOUND,
        "lo ajeno no existe para él"
    );

    // Su horario (DA-08) y un bloqueo para mañana (E2E-BAR-06).
    let horario = json!({ "franjas": [{ "dia_semana": 6, "inicio": "08:00", "fin": "14:00" }, { "dia_semana": 1, "inicio": "10:00", "fin": "18:00" }] });
    let r = pedir(
        &app,
        Method::PUT,
        &format!("/api/v1/barberos/{santiago}/horario"),
        Some(&barbero),
        Some(horario),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["total_horas_semana"].as_f64()),
        (StatusCode::OK, Some(14.0))
    );
    let manana = fecha(1).await;
    let bloqueo = json!({ "fecha": manana, "desde": "14:00", "hasta": "16:00", "motivo": "cita_medica" });
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/bloqueos"),
        Some(&barbero),
        Some(bloqueo),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let cruzado = json!({ "fecha": manana, "desde": "15:00", "hasta": "17:00", "motivo": "otro" });
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/bloqueos"),
        Some(&admin),
        Some(cruzado),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("bloqueo_solapado"))
    );

    // Horario de atención calculado con las franjas (RF-CFG-03).
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/negocio/horario-atencion",
        Some(&barbero),
        None,
    )
    .await;
    assert_eq!(
        (r.json[0]["abre"].as_str(), r.json[5]["cierra"].as_str()),
        (Some("10:00:00"), Some("14:00:00"))
    );

    // Vacaciones desde hoy: el estado cambia por la fecha y vuelve a «Activo» al anularlas.
    let (hoy, en_tres) = (fecha(0).await, fecha(3).await);
    let ausencia = json!({ "tipo": "vacaciones", "desde": hoy, "hasta": en_tres, "observacion": "Vacaciones de fin de año" });
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/ausencias"),
        Some(&admin),
        Some(ausencia.clone()),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let ausencia_id = r.json["id"].clone();
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/ausencias"),
        Some(&admin),
        Some(ausencia),
    )
    .await;
    assert_eq!(r.json["codigo"], "ausencia_solapada");
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/barberos?estado=vacaciones",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (
            r.json["meta"]["ausencias_temporales"].as_u64(),
            r.json["datos"][0]["estado"]["hasta"].as_str()
        ),
        (Some(1), Some(en_tres.as_str()))
    );
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/ausencias/{ausencia_id}/anular"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (r.estado, r.json["anulada"].as_bool()),
        (StatusCode::OK, Some(true))
    );
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/barberos?q=SANTIAGO",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["datos"][0]["estado"]["codigo"], "activo");

    // Foto: vertical sí, horizontal no; la pública se sirve en /media.
    let (tipo, cuerpo) = multipart(&[("foto", Some("santi.png"), &png(400, 300))]);
    let (estado, _, _) = pedir_bytes(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/foto"),
        Some(&admin),
        &tipo,
        cuerpo,
    )
    .await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
    let (tipo, cuerpo) = multipart(&[("foto", Some("santi.png"), &png(300, 400))]);
    let (estado, _, cuerpo) = pedir_bytes(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/foto"),
        Some(&admin),
        &tipo,
        cuerpo,
    )
    .await;
    assert_eq!(estado, StatusCode::OK);
    let foto_url = serde_json::from_slice::<Value>(&cuerpo).unwrap()["foto_url"]
        .as_str()
        .unwrap()
        .to_owned();
    let (estado, cabeceras, _) = pedir_bytes(&app, Method::GET, &foto_url, None, "", Vec::new()).await;
    assert_eq!(
        (estado, cabeceras[header::CONTENT_TYPE].to_str().unwrap()),
        (StatusCode::OK, "image/png")
    );

    // Documentos privados: carga, expediente incompleto y descarga con URL firmada.
    let (tipo, cuerpo) = multipart(&[
        ("tipo", None, b"contrato"),
        ("archivo", Some("contrato.pdf"), b"%PDF-1.4 contrato"),
    ]);
    let (estado, _, cuerpo) = pedir_bytes(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/documentos"),
        Some(&admin),
        &tipo,
        cuerpo,
    )
    .await;
    assert_eq!(
        estado,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&cuerpo)
    );
    let documento = serde_json::from_slice::<Value>(&cuerpo).unwrap()["id"].clone();
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/barberos/{santiago}/documentos"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["faltantes"], json!(["identidad", "hoja_de_vida"]));
    let ruta = format!("/api/v1/barberos/{santiago}/documentos/{documento}/descarga");
    let (estado, cabeceras, _) = pedir_bytes(&app, Method::GET, &ruta, Some(&admin), "", Vec::new()).await;
    assert_eq!(estado, StatusCode::SEE_OTHER);
    let firmada = cabeceras[header::LOCATION].to_str().unwrap().to_owned();
    let (estado, _, contenido) = pedir_bytes(&app, Method::GET, &firmada, None, "", Vec::new()).await;
    assert_eq!(
        (estado, contenido.as_slice()),
        (StatusCode::OK, &b"%PDF-1.4 contrato"[..])
    );

    // Datos privados cifrados en reposo (RNF-LEG-04).
    let privados = json!({ "grupo_sanguineo": "O+", "medio_pago": "nequi", "numero_cuenta": "3001112233", "emergencia": { "nombre": "Marta Ruiz", "parentesco": "madre_padre", "celular": "3001112233" } });
    let r = pedir(
        &app,
        Method::PUT,
        &format!("/api/v1/barberos/{santiago}/datos-privados"),
        Some(&admin),
        Some(privados),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["grupo_sanguineo"].as_str()),
        (StatusCode::OK, Some("O+")),
        "{}",
        r.json
    );
    let cifrado: Vec<u8> = sqlx::query_scalar("SELECT numero_cuenta_cifrado FROM barberos.datos_privados")
        .fetch_one(&base.pool)
        .await
        .unwrap();
    assert!(
        !cifrado.windows(10).any(|w| w == b"3001112233"),
        "el número de cuenta no queda en claro"
    );
    assert_eq!(
        pedir(
            &app,
            Method::GET,
            &format!("/api/v1/barberos/{santiago}/datos-privados"),
            Some(&admin),
            None
        )
        .await
        .json["numero_cuenta"],
        "3001112233"
    );

    // Orden del equipo con un segundo barbero (RF-BAR-19).
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/barberos",
        Some(&admin),
        Some(registro("Mateo", "1020304051", "mateo@diamante.co", vec![])),
    )
    .await;
    let mateo = r.json["id"].as_i64().unwrap();
    let r = pedir(
        &app,
        Method::PUT,
        "/api/v1/barberos/orden",
        Some(&admin),
        Some(json!({ "barbero_ids": [mateo, santiago] })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::NO_CONTENT);
    let r = pedir(&app, Method::GET, "/api/v1/barberos", Some(&admin), None).await;
    assert_eq!(r.json["datos"][0]["id"].as_i64(), Some(mateo));

    // Retiro hoy (E2E-BAR-04): pierde el acceso y deja de ofrecer servicios; su historial sigue.
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{santiago}/retiro"),
        Some(&admin),
        Some(json!({ "fecha": hoy, "motivo": "renuncia_voluntaria" })),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["estado"]["codigo"].as_str()),
        (StatusCode::OK, Some("inactivo")),
        "{}",
        r.json
    );
    assert_eq!(
        pedir(&app, Method::GET, "/api/v1/yo", Some(&barbero), None)
            .await
            .estado,
        StatusCode::UNAUTHORIZED
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/servicios/{}", corte["id"]),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["barberos_que_lo_ofrecen"], 0);
    let r = pedir(
        &app,
        Method::PUT,
        &format!("/api/v1/barberos/{santiago}/visibilidad"),
        Some(&admin),
        Some(json!({ "mostrar_en_landing": true, "recibe_citas_en_linea": true, "acceso_panel": true })),
    )
    .await;
    assert_eq!(r.json["codigo"], "barbero_retirado");

    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/auditoria?entidad=barbero&entidad_id={santiago}&por_pagina=50"),
        Some(&admin),
        None,
    )
    .await;
    let acciones: Vec<&str> = r.json["datos"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["accion"].as_str())
        .collect();
    for accion in [
        "registrar_barbero",
        "cambiar_horario",
        "crear_bloqueo",
        "registrar_ausencia",
        "cambiar_foto",
        "cargar_documento",
        "actualizar_datos_privados",
        "retirar_barbero",
    ] {
        assert!(acciones.contains(&accion), "falta {accion} en {acciones:?}");
    }
    let _ = std::fs::remove_dir_all(&raiz);
    base.eliminar().await;
}

// ---------------------------------------------------------------------------------------------
// Fase 3: verificación, reserva en línea, gestión por enlace y agenda del panel
// ---------------------------------------------------------------------------------------------

/// Variables del último WhatsApp en cola para `celular` con esa plantilla.
async fn whatsapp(base: &BaseDePrueba, celular: &str, plantilla: &str) -> Value {
    sqlx::query_scalar(
        "SELECT variables FROM notificaciones.mensaje
         WHERE canal = 'whatsapp' AND destinatario = $1 AND plantilla = $2
         ORDER BY id DESC LIMIT 1",
    )
    .bind(celular)
    .bind(plantilla)
    .fetch_one(&base.pool)
    .await
    .unwrap_or_else(|e| panic!("sin WhatsApp {plantilla} para {celular}: {e}"))
}

/// Pide el código, lo lee de la cola (WhatsApp simulado) y lo confirma.
async fn verificar(app: &Router, base: &BaseDePrueba, celular: &str) -> Value {
    let r = pedir(
        app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": celular })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(
        r.json.as_object().unwrap().len(),
        3,
        "sin datos del cliente: {}",
        r.json
    );
    let codigo = whatsapp(base, celular, "codigo_verificacion").await["codigo"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = pedir(
        app,
        Method::POST,
        &format!("/api/v1/publico/verificaciones/{}/confirmar", r.json["id"]),
        None,
        Some(json!({ "codigo": codigo })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    r.json
}

fn token_de(enlace: &str) -> String {
    enlace.rsplit('/').next().unwrap().to_owned()
}

#[tokio::test]
async fn reserva_en_linea_y_agenda_de_punta_a_punta() {
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
    let consulta = |sql: &'static str| {
        let pool = base.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let hoy = consulta("SELECT (now() AT TIME ZONE 'America/Bogota')::date::text").await;
    let manana = consulta("SELECT ((now() AT TIME ZONE 'America/Bogota')::date + 1)::text").await;
    // El primer día hábil (lunes a sábado) a partir de pasado mañana: lejos de la anticipación
    // mínima y del aviso de cancelación, dentro de la ventana de 30 días.
    let dia = consulta(
        "SELECT d::date::text FROM generate_series((now() AT TIME ZONE 'America/Bogota')::date + 2,
                                                    (now() AT TIME ZONE 'America/Bogota')::date + 8,
                                                    interval '1 day') d
         WHERE extract(isodow FROM d) <= 6 ORDER BY d LIMIT 1",
    )
    .await;
    let a_las = |hora: &str| format!("{dia}T{hora}:00-05:00");

    // Administradora, política de datos, catálogo y equipo.
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
    let politica = json!({ "tipo": "tratamiento_datos", "version": "1.0", "url": "https://diamante.co/politicas/datos" });
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/negocio/politicas",
        Some(&admin),
        Some(politica),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let politica = r.json["id"].as_i64().unwrap();
    let servicio = |nombre: &str, precio: i64| json!({ "categoria_id": 1, "nombre": nombre, "descripcion": "Servicio.", "precio": precio, "duracion_min": 30 });
    let corte = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        Some(&admin),
        Some(servicio("Corte clásico", 35_000)),
    )
    .await
    .json["id"]
        .as_i64()
        .unwrap();
    let cejas = pedir(
        &app,
        Method::POST,
        "/api/v1/servicios",
        Some(&admin),
        Some(servicio("Diseño de cejas", 10_000)),
    )
    .await
    .json["id"]
        .as_i64()
        .unwrap();
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/barberos",
        Some(&admin),
        Some(registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![json!(corte), json!(cejas)],
        )),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let santiago = r.json["id"].as_i64().unwrap();
    let mateo = pedir(
        &app,
        Method::POST,
        "/api/v1/barberos",
        Some(&admin),
        Some(registro(
            "Mateo",
            "1020304051",
            "mateo@diamante.co",
            vec![json!(corte)],
        )),
    )
    .await
    .json["id"]
        .as_i64()
        .unwrap();
    // Mateo está de vacaciones hoy y mañana; vuelve antes del día de las citas.
    let vacaciones =
        json!({ "tipo": "vacaciones", "desde": hoy, "hasta": manana, "observacion": "Descanso." });
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{mateo}/ausencias"),
        Some(&admin),
        Some(vacaciones),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);

    // E2E-PUB-01/02/04: la landing con datos reales y caché corta.
    let r = pedir(&app, Method::GET, "/api/v1/publico/landing", None, None).await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(r.cabeceras[header::CACHE_CONTROL], "public, max-age=60");
    assert_eq!(r.json["servicios"].as_array().unwrap().len(), 2);
    assert_eq!(r.json["politicas"][0]["tipo"], "tratamiento_datos");
    let equipo: Vec<(i64, bool, Value)> = r.json["barberos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["id"].as_i64().unwrap(),
                b["agendable"].as_bool().unwrap(),
                b["de_vacaciones_hasta"].clone(),
            )
        })
        .collect();
    assert_eq!(
        equipo,
        vec![(santiago, true, Value::Null), (mateo, false, json!(manana))]
    );
    let nuevo_precio = servicio("Corte clásico", 38_000);
    let r = pedir(
        &app,
        Method::PUT,
        &format!("/api/v1/servicios/{corte}"),
        Some(&admin),
        Some(nuevo_precio),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let r = pedir(&app, Method::GET, "/api/v1/publico/landing", None, None).await;
    let precio = r.json["servicios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == corte)
        .unwrap()["precio"]
        .clone();
    assert_eq!(precio, 38_000);

    // E2E-RES-03: quién puede atender cada servicio.
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/publico/barberos?servicio_id={cejas}"),
        None,
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let opciones: Vec<(bool, Option<&str>)> = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["disponible"].as_bool().unwrap(), o["motivo"].as_str()))
        .collect();
    assert_eq!(
        opciones,
        vec![(true, None), (false, Some("No realiza este servicio"))]
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/publico/barberos?servicio_id={corte}"),
        None,
        None,
    )
    .await;
    assert!(
        r.json[1]["motivo"]
            .as_str()
            .unwrap()
            .starts_with("De vacaciones hasta el "),
        "{}",
        r.json
    );

    // Calendario del mes y horas del día (RF-RES-04, RF-RES-05).
    let mes = &dia[..7];
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/publico/disponibilidad/calendario?servicio_id={corte}&mes={mes}"),
        None,
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let del_dia = r.json["dias"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["fecha"] == dia.as_str())
        .unwrap()
        .clone();
    assert_eq!(del_dia["estado"], "disponible", "{del_dia}");
    let horas = |barbero: Option<i64>| {
        let app = app.clone();
        let ruta = format!(
            "/api/v1/publico/disponibilidad/horas?servicio_id={corte}&fecha={dia}{}",
            barbero.map(|b| format!("&barbero_id={b}")).unwrap_or_default()
        );
        async move {
            let r = pedir(&app, Method::GET, &ruta, None, None).await;
            assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
            ["manana", "tarde", "noche"]
                .iter()
                .flat_map(|franja| r.json[franja].as_array().unwrap().clone())
                .map(|h| {
                    (
                        h["hora"].as_str().unwrap().to_owned(),
                        h["barbero_id"].as_i64().unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        }
    };
    let libres = horas(None).await;
    assert_eq!(
        libres.first().unwrap().1,
        santiago,
        "«Cualquier barbero» toma al primero del equipo"
    );
    assert_eq!(
        libres.len(),
        20,
        "de 9:00 a 6:30 p. m. cada 30 minutos: {libres:?}"
    );

    // E2E-RES-05/06: validaciones, código incorrecto y espera para reenviar.
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": "2001112233" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": "3002223344" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::ACCEPTED, "{}", r.json);
    let id_bruno = r.json["id"].clone();
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": "3002223344" })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert!(r.cabeceras.contains_key(header::RETRY_AFTER));
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/publico/verificaciones/{id_bruno}/confirmar"),
        None,
        Some(json!({ "codigo": "000000" })),
    )
    .await;
    let codigo_bruno = whatsapp(&base, "3002223344", "codigo_verificacion").await["codigo"]
        .as_str()
        .unwrap()
        .to_owned();
    if codigo_bruno != "000000" {
        assert_eq!(
            (r.estado, r.json["codigo"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("codigo_incorrecto")),
            "{}",
            r.json
        );
    }
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/publico/verificaciones/{id_bruno}/confirmar"),
        None,
        Some(json!({ "codigo": codigo_bruno })),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["cliente"].is_null()),
        (StatusCode::OK, true),
        "{}",
        r.json
    );
    let comprobante_bruno = r.json["comprobante"].as_str().unwrap().to_owned();

    // E2E-RES-01: cliente nueva, «Cualquier barbero», confirmada al instante.
    let ana = verificar(&app, &base, "3001112233").await;
    assert!(ana["cliente"].is_null(), "cliente nueva: {ana}");
    let reserva = |comprobante: &str, barbero: Option<i64>, inicio: &str, nombre: &str| {
        json!({
            "comprobante": comprobante, "servicio_id": corte, "barbero_id": barbero, "inicio": inicio,
            "cliente": { "nombre": nombre, "correo": null, "fecha_nacimiento": null },
            "autoriza_tratamiento_datos": true, "politica_version_id": politica
        })
    };
    let sin_autorizar = {
        let mut cuerpo = reserva(
            ana["comprobante"].as_str().unwrap(),
            None,
            &a_las("10:00"),
            "Ana Torres",
        );
        cuerpo["autoriza_tratamiento_datos"] = json!(false);
        cuerpo
    };
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(sin_autorizar),
    )
    .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(reserva(
            ana["comprobante"].as_str().unwrap(),
            None,
            &a_las("10:00"),
            "Ana Torres",
        )),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let cita_ana = r.json["cita"].clone();
    assert_eq!(
        (
            cita_ana["estado"].as_str(),
            cita_ana["origen"].as_str(),
            cita_ana["barbero"]["id"].as_i64(),
            cita_ana["barbero"]["asignacion_automatica"].as_bool()
        ),
        (Some("confirmada"), Some("en_linea"), Some(santiago), Some(true))
    );
    assert_eq!(cita_ana["precio"], 38_000, "precio vigente al agendar");
    let enlace_ana = r.json["enlace_gestion"].as_str().unwrap().to_owned();
    assert!(enlace_ana.starts_with(&format!("{SITIO}/cita/")), "{enlace_ana}");
    assert!(
        r.json["calendario_google"]
            .as_str()
            .unwrap()
            .contains("ctz=America%2FBogota"),
        "{}",
        r.json
    );
    assert!(
        r.cabeceras[header::LOCATION]
            .to_str()
            .unwrap()
            .starts_with("/api/v1/publico/citas/gestion/")
    );
    let confirmacion = whatsapp(&base, "3001112233", "cita_confirmada").await;
    assert_eq!(
        (confirmacion["barbero"].as_str(), confirmacion["enlace"].as_str()),
        (Some("Santiago Ruiz"), Some(enlace_ana.as_str()))
    );
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(reserva(
            ana["comprobante"].as_str().unwrap(),
            None,
            &a_las("11:00"),
            "Ana Torres",
        )),
    )
    .await;
    assert_eq!(
        r.estado,
        StatusCode::GONE,
        "el comprobante sirve para una sola cita: {}",
        r.json
    );

    // E2E-RES-07: la hora ya no está; alternativas y el mismo comprobante sirve para otra.
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(reserva(
            &comprobante_bruno,
            Some(santiago),
            &a_las("10:00"),
            "Bruno Díaz",
        )),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["codigo"].as_str()),
        (StatusCode::CONFLICT, Some("conflicto_horario")),
        "{}",
        r.json
    );
    let alternativas: Vec<&str> = r.json["alternativas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        alternativas,
        vec![a_las("09:00"), a_las("09:30"), a_las("10:30")],
        "las tres más cercanas, en orden"
    );
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(reserva(
            &comprobante_bruno,
            Some(santiago),
            &a_las("10:30"),
            "Bruno Díaz",
        )),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let cita_bruno = r.json["cita"]["id"].as_i64().unwrap();
    let enlace_bruno = r.json["enlace_gestion"].as_str().unwrap().to_owned();
    assert!(
        !horas(Some(santiago))
            .await
            .iter()
            .any(|(h, _)| h == "10:00" || h == "10:30")
    );

    // E2E-RES-02: el cliente habitual ve sus datos solo después de verificar.
    // (pasado el minuto de espera para pedir otro código)
    sqlx::query("UPDATE verificacion.codigo SET creado_en = creado_en - interval '2 minutes'")
        .execute(&base.pool)
        .await
        .unwrap();
    let bruno_otra_vez = verificar(&app, &base, "3002223344").await;
    assert_eq!(
        bruno_otra_vez["cliente"]["nombre"], "Bruno Díaz",
        "{bruno_otra_vez}"
    );

    // Gestión por enlace (CU-10): ver, recordatorios, reprogramar y cancelar (E2E-RES-08).
    let gestion = format!("/api/v1/publico/citas/gestion/{}", token_de(&enlace_ana));
    let r = pedir(&app, Method::GET, &gestion, None, None).await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(r.cabeceras[header::CACHE_CONTROL], "private, no-store");
    assert_eq!(
        (
            r.json["puede_cancelar"].as_bool(),
            r.json["cancelacion_seria_tardia"].as_bool(),
            r.json["recordatorios_activos"].as_bool()
        ),
        (Some(true), Some(false), Some(true))
    );
    assert!(
        r.json["cita"].get("cliente").is_none(),
        "el enlace no expone datos de contacto"
    );
    let r = pedir(
        &app,
        Method::PUT,
        &format!("{gestion}/recordatorios"),
        None,
        Some(json!({ "activos": false })),
    )
    .await;
    assert_eq!(r.json["recordatorios_activos"], false, "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        &format!("{gestion}/reprogramar"),
        None,
        Some(json!({ "inicio": a_las("10:30"), "barbero_id": santiago })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CONFLICT, "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        &format!("{gestion}/reprogramar"),
        None,
        Some(json!({ "inicio": a_las("11:00"), "barbero_id": null })),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["cita"]["inicio"].as_str()),
        (StatusCode::OK, Some(a_las("11:00").as_str())),
        "{}",
        r.json
    );
    assert!(
        horas(Some(santiago)).await.iter().any(|(h, _)| h == "10:00"),
        "la hora previa queda libre"
    );
    let r = pedir(&app, Method::POST, &format!("{gestion}/cancelar"), None, None).await;
    assert_eq!(
        (
            r.estado,
            r.json["cita"]["estado"].as_str(),
            r.json["cita"]["cancelacion_tardia"].as_bool()
        ),
        (StatusCode::OK, Some("cancelada"), Some(false)),
        "{}",
        r.json
    );
    whatsapp(&base, "3001112233", "cita_cancelada").await;
    assert!(
        horas(Some(santiago)).await.iter().any(|(h, _)| h == "11:00"),
        "la hora vuelve a ofrecerse"
    );
    let r = pedir(&app, Method::POST, &format!("{gestion}/cancelar"), None, None).await;
    assert_eq!(r.estado, StatusCode::CONFLICT, "ya estaba cancelada: {}", r.json);

    // E2E-AGE-01: tablero del día con una columna por barbero.
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/agenda/tablero?fecha={dia}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let columnas: Vec<(i64, usize)> = r.json["barberos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["barbero_id"].as_i64().unwrap(),
                c["citas"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(columnas, vec![(santiago, 2), (mateo, 0)], "{}", r.json);
    assert_eq!(
        (r.json["total_citas"].as_u64(), r.json["por_atender"].as_u64()),
        (Some(1), Some(1)),
        "{}",
        r.json
    );
    assert_eq!(
        pedir(
            &app,
            Method::GET,
            &format!("/api/v1/agenda/tablero?fecha={dia}"),
            None,
            None
        )
        .await
        .estado,
        StatusCode::UNAUTHORIZED
    );

    // E2E-AGE-02: cita creada desde el panel para un cliente que llamó.
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/agenda/citas",
        Some(&admin),
        Some(json!({
            "cliente_nuevo": { "nombre": "Carlos Pérez", "celular": "3003334455" },
            "servicio_id": corte, "barbero_id": mateo, "inicio": a_las("15:00"), "nota": "Llamó por teléfono."
        })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    assert_eq!(
        (r.json["estado"].as_str(), r.json["origen"].as_str()),
        (Some("sin_confirmar"), Some("panel")),
        "{}",
        r.json
    );
    let cita_carlos = r.json["id"].as_i64().unwrap();
    let carlos = r.json["cliente"]["id"].as_i64().unwrap();
    whatsapp(&base, "3003334455", "cita_confirmada").await;
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{cita_carlos}/confirmar"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["estado"], "confirmada", "{}", r.json);

    // E2E-AGE-03: reprogramación a otra hora y otro barbero; el enlace anterior deja de servir.
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{cita_bruno}/reprogramar"),
        Some(&admin),
        Some(json!({ "inicio": a_las("16:00"), "barbero_id": mateo })),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["barbero"]["id"].as_i64()),
        (StatusCode::OK, Some(mateo)),
        "{}",
        r.json
    );
    let eventos: Vec<&str> = r.json["historial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["evento"].as_str().unwrap())
        .collect();
    assert_eq!(eventos, vec!["creada", "reasignada"]);
    let aviso = whatsapp(&base, "3002223344", "cita_reprogramada").await;
    let nuevo_enlace = aviso["enlace"].as_str().unwrap().to_owned();
    assert_ne!(nuevo_enlace, enlace_bruno);
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/publico/citas/gestion/{}", token_de(&enlace_bruno)),
        None,
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::GONE, "{}", r.json);

    // E2E-RES-09: a dos horas de la cita, la cancelación es tardía y no es una inasistencia.
    sqlx::query("UPDATE agenda.cita SET inicio = now() + interval '2 hours', fin = now() + interval '150 minutes' WHERE id = $1")
        .bind(cita_bruno)
        .execute(&base.pool)
        .await
        .unwrap();
    let gestion_bruno = format!("/api/v1/publico/citas/gestion/{}", token_de(&nuevo_enlace));
    let r = pedir(&app, Method::GET, &gestion_bruno, None, None).await;
    assert_eq!(r.json["cancelacion_seria_tardia"], true, "{}", r.json);
    let r = pedir(
        &app,
        Method::POST,
        &format!("{gestion_bruno}/cancelar"),
        None,
        None,
    )
    .await;
    assert_eq!(
        (
            r.json["cita"]["estado"].as_str(),
            r.json["cita"]["cancelacion_tardia"].as_bool()
        ),
        (Some("cancelada"), Some(true)),
        "{}",
        r.json
    );

    // Conflictos antes de registrar una ausencia (RF-BAR-13).
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/agenda/conflictos",
        Some(&admin),
        Some(json!({ "barbero_id": mateo, "ausencia": { "desde": dia, "hasta": dia } })),
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    let afectadas: Vec<i64> = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    assert_eq!(afectadas, vec![cita_carlos]);

    // E2E-AGE-05: no se marca inasistencia en el futuro; en una cita de hoy, sí, y suma en la ficha.
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{cita_carlos}/inasistencia"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::CONFLICT, "{}", r.json);
    sqlx::query("UPDATE agenda.cita SET inicio = now() - interval '1 hour', fin = now() - interval '30 minutes' WHERE id = $1")
        .bind(cita_carlos)
        .execute(&base.pool)
        .await
        .unwrap();
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/agenda/pendientes-de-cierre",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.estado, StatusCode::OK, "{}", r.json);
    assert_eq!(
        r.json
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![cita_carlos]
    );
    let r = pedir(
        &app,
        Method::POST,
        &format!("/api/v1/agenda/citas/{cita_carlos}/inasistencia"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (r.estado, r.json["estado"].as_str()),
        (StatusCode::OK, Some("no_asistio")),
        "{}",
        r.json
    );
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/clientes/{carlos}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["inasistencias"], 1, "{}", r.json);

    // Clientes: búsqueda rápida y listado; la autorización de datos quedó registrada en línea.
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/clientes/buscar?q=bruno",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json.as_array().unwrap().len(), 1, "{}", r.json);
    let bruno = r.json[0]["id"].as_i64().unwrap();
    let r = pedir(
        &app,
        Method::GET,
        &format!("/api/v1/clientes/{bruno}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        (
            r.json["inasistencias"].as_i64(),
            r.json["autorizaciones"][0]["politica_version_id"].as_i64()
        ),
        (Some(0), Some(politica)),
        "{}",
        r.json
    );
    let r = pedir(&app, Method::GET, "/api/v1/clientes", Some(&admin), None).await;
    assert_eq!(r.json["total"], 3, "{}", r.json);

    // Configuración de la agenda (RF-CFG-02), validada y auditada.
    let r = pedir(
        &app,
        Method::GET,
        "/api/v1/agenda/configuracion",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["anticipacion_minima_min"], 120, "{}", r.json);
    let mut cambio = r.json.clone();
    cambio["intervalo_min"] = json!(45);
    let r = pedir(
        &app,
        Method::PUT,
        "/api/v1/agenda/configuracion",
        Some(&admin),
        Some(cambio.clone()),
    )
    .await;
    assert_eq!(r.estado, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
    cambio["intervalo_min"] = json!(15);
    let r = pedir(
        &app,
        Method::PUT,
        "/api/v1/agenda/configuracion",
        Some(&admin),
        Some(cambio),
    )
    .await;
    assert_eq!(
        (r.estado, r.json["intervalo_min"].as_i64()),
        (StatusCode::OK, Some(15)),
        "{}",
        r.json
    );
    assert_eq!(
        horas(Some(mateo)).await.len(),
        39,
        "con intervalo de 15 minutos: 9:00 a 6:30 p. m."
    );

    base.eliminar().await;
}
