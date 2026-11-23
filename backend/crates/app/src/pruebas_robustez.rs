//! Robustez ante entradas hostiles (búsqueda de errores de la fase 8). Cada ruta de la API se
//! llama con un cuerpo válido en el que cada campo, por turnos, se cambia por un valor hostil
//! (el carácter nulo, textos enormes, fechas de otros milenios, números fuera de rango, tipos
//! equivocados) o se quita; cada parámetro de la URL y cada identificador de la ruta también se
//! cambian, y cada cuerpo se manda además roto (JSON mal formado, sin tipo, anidado sin fin).
//!
//! Ninguna respuesta puede ser `5xx` (salvo `503 pdf_no_disponible`: en la prueba no hay
//! Gotenberg), todo error debe venir en *Problem Details* (RFC 9457) y nada puede entrar en
//! pánico. La primera pasada encontró, y quedaron corregidos:
//!
//! - el carácter nulo daba `500` en cualquier texto, también en el inicio de sesión y en lo
//!   público (PostgreSQL no lo admite);
//! - una fecha como `+262142-12-31` tumbaba la petición con un pánico de chrono en la
//!   disponibilidad pública, la lista de citas y el tablero, y daba `500` en el dashboard;
//! - un bloqueo en el año 262142 se guardaba sin más.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{Datelike, Days, Weekday};
use nucleo::Actor;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};

use crate::composicion::Composicion;
use crate::pruebas_postgres::{
    CLAVE, SECRETO_APP, SITIO, entrar, firmado, multipart, pedir, pedir_bytes, png, registro,
    token_del_correo, verificar, webhook,
};
use crate::{enrutador, migraciones};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quien {
    Admin,
    Barbero,
    Nadie,
}

#[derive(Debug, Clone)]
struct Plantilla {
    metodo: Method,
    /// Ruta completa con sus identificadores y su consulta.
    ruta: String,
    cuerpo: Option<Value>,
    quien: Quien,
}

fn p(metodo: Method, ruta: impl Into<String>, cuerpo: Option<Value>, quien: Quien) -> Plantilla {
    let ruta = ruta.into();
    let ruta = if ruta.starts_with("/media") {
        ruta
    } else {
        format!("/api/v1{ruta}")
    };
    Plantilla {
        metodo,
        ruta,
        cuerpo,
        quien,
    }
}

/// Valores hostiles para cualquier nodo de un cuerpo JSON.
fn hostiles() -> Vec<Value> {
    vec![
        Value::Null,
        json!("\u{0}"),
        json!("x".repeat(5_000)),
        json!(""),
        json!("+262142-12-31"),
        json!("-262143-01-01T00:00:00Z"),
        json!("2026-02-30"),
        json!("😀\u{202e}<script>'\"--"),
        json!(-1),
        json!(i64::MAX),
        json!(u64::MAX),
        json!(1e300),
        json!(0.5),
        json!(2_147_483_648_i64),
        json!(32_768),
        json!(true),
        json!({}),
        json!([]),
    ]
}

/// Valores hostiles para un parámetro de la URL (ya codificados).
fn hostiles_url() -> Vec<String> {
    let mut valores: Vec<String> = [
        "%00",
        "%2B262142-12-31",
        "-262143-01-01",
        "2026-02-30",
        "-1",
        "99999999999999999999",
        "",
        "%F0%9F%98%80",
        "1e309",
        "%27%20OR%201%3D1--",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    valores.push("x".repeat(3_000));
    valores
}

/// Valores hostiles para un identificador de la ruta.
fn hostiles_id() -> Vec<String> {
    [
        "0",
        "-1",
        "9223372036854775807",
        "99999999999999999999",
        "abc",
        "%00",
        "1.5",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// Ruta, partes del formulario (campo, nombre de archivo, contenido) y qué se prueba.
type FormularioHostil<'a> = (String, Vec<(&'a str, Option<&'a str>, &'a [u8])>, &'a str);

#[derive(Debug, Clone)]
enum Paso {
    Campo(String),
    Indice(usize),
}

/// Ruta a cada nodo del JSON (sin la raíz). En los arreglos solo se baja por el primero.
fn nodos(valor: &Value, actual: &mut Vec<Paso>, todos: &mut Vec<Vec<Paso>>) {
    match valor {
        Value::Object(campos) => {
            for (clave, hijo) in campos {
                actual.push(Paso::Campo(clave.clone()));
                todos.push(actual.clone());
                nodos(hijo, actual, todos);
                actual.pop();
            }
        }
        Value::Array(elementos) => {
            if let Some(primero) = elementos.first() {
                actual.push(Paso::Indice(0));
                todos.push(actual.clone());
                nodos(primero, actual, todos);
                actual.pop();
            }
        }
        _ => {}
    }
}

/// Copia de `valor` con el nodo de `ruta` cambiado por `nuevo`, o quitado.
fn cambiado(valor: &Value, ruta: &[Paso], nuevo: Option<&Value>) -> Value {
    let mut copia = valor.clone();
    let (ultimo, padres) = ruta.split_last().unwrap();
    let mut nodo = &mut copia;
    for paso in padres {
        nodo = match paso {
            Paso::Campo(clave) => &mut nodo[clave.as_str()],
            Paso::Indice(i) => &mut nodo[*i],
        };
    }
    match (ultimo, nuevo) {
        (Paso::Campo(clave), Some(v)) => nodo[clave.as_str()] = v.clone(),
        (Paso::Campo(clave), None) => {
            nodo.as_object_mut().unwrap().remove(clave);
        }
        (Paso::Indice(i), Some(v)) => nodo[*i] = v.clone(),
        (Paso::Indice(i), None) => {
            nodo.as_array_mut().unwrap().remove(*i);
        }
    }
    copia
}

fn describir(ruta: &[Paso]) -> String {
    ruta.iter()
        .map(|p| match p {
            Paso::Campo(c) => c.clone(),
            Paso::Indice(i) => format!("[{i}]"),
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn recortar(texto: &str) -> String {
    texto.chars().take(160).collect()
}

/// Variantes de la consulta: cada parámetro con cada valor hostil, sin él y con uno de más.
fn variantes_de_consulta(ruta: &str) -> Vec<(String, String)> {
    let Some((base, consulta)) = ruta.split_once('?') else {
        return vec![(format!("{ruta}?inventado=1"), "parámetro de más".into())];
    };
    let pares: Vec<(&str, &str)> = consulta
        .split('&')
        .filter_map(|par| par.split_once('='))
        .collect();
    let armar = |pares: &[(&str, String)]| {
        format!(
            "{base}?{}",
            pares
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&")
        )
    };
    let mut variantes = vec![(format!("{ruta}&inventado=1"), "parámetro de más".into())];
    for (i, (clave, _)) in pares.iter().enumerate() {
        for hostil in hostiles_url() {
            let nuevos: Vec<(&str, String)> = pares
                .iter()
                .enumerate()
                .map(|(j, (k, v))| (*k, if i == j { hostil.clone() } else { (*v).to_owned() }))
                .collect();
            variantes.push((armar(&nuevos), format!("{clave}={}", recortar(&hostil))));
        }
        let sin: Vec<(&str, String)> = pares
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, (k, v))| (*k, (*v).to_owned()))
            .collect();
        variantes.push((armar(&sin), format!("sin {clave}")));
    }
    variantes
}

/// Variantes de la ruta: cada segmento numérico (un identificador) con cada valor hostil.
fn variantes_de_ruta(ruta: &str, tokens: &[String]) -> Vec<(String, String)> {
    let (camino, consulta) = match ruta.split_once('?') {
        Some((c, q)) => (c, format!("?{q}")),
        None => (ruta, String::new()),
    };
    let segmentos: Vec<&str> = camino.split('/').collect();
    let mut variantes = Vec::new();
    for (i, segmento) in segmentos.iter().enumerate() {
        let es_id = !segmento.is_empty() && segmento.bytes().all(|b| b.is_ascii_digit()) && i > 3;
        let es_token = tokens.iter().any(|t| t == segmento);
        if !es_id && !es_token {
            continue;
        }
        let mut valores = hostiles_id();
        if es_token {
            valores.extend(["x".repeat(3_000), "%2E%2E".into(), "a%2Fb".into()]);
        }
        for valor in valores {
            let mut nuevos = segmentos.clone();
            nuevos[i] = &valor;
            variantes.push((
                format!("{}{consulta}", nuevos.join("/")),
                format!("segmento {i} = {}", recortar(&valor)),
            ));
        }
    }
    variantes
}

struct Fuzz {
    app: Router,
    admin: String,
    barbero: String,
    fallas: Vec<String>,
    peticiones: usize,
}

impl Fuzz {
    fn sesion(&self, quien: Quien) -> Option<String> {
        match quien {
            Quien::Admin => Some(self.admin.clone()),
            Quien::Barbero => Some(self.barbero.clone()),
            Quien::Nadie => None,
        }
    }

    /// Revisa una respuesta: sin `5xx` (salvo el PDF sin Gotenberg) y errores en *Problem Details*.
    fn revisar(&mut self, que: &str, estado: StatusCode, cuerpo: &Value) {
        self.peticiones += 1;
        let pdf_sin_gotenberg =
            estado == StatusCode::SERVICE_UNAVAILABLE && cuerpo["codigo"] == "pdf_no_disponible";
        if estado.is_server_error() && !pdf_sin_gotenberg {
            self.fallas
                .push(format!("{que} → {estado} {}", recortar(&cuerpo.to_string())));
        } else if estado.is_client_error()
            && (cuerpo["status"].as_u64() != Some(u64::from(estado.as_u16())) || !cuerpo["type"].is_string())
        {
            self.fallas.push(format!(
                "{que} → {estado} sin Problem Details: {}",
                recortar(&cuerpo.to_string())
            ));
        }
    }

    async fn json(&mut self, metodo: &Method, ruta: &str, quien: Quien, cuerpo: Option<Value>, que: &str) {
        let sesion = self.sesion(quien);
        let r = pedir(&self.app, metodo.clone(), ruta, sesion.as_deref(), cuerpo).await;
        self.revisar(que, r.estado, &r.json);
    }

    async fn crudo(
        &mut self,
        metodo: &Method,
        ruta: &str,
        quien: Quien,
        tipo: &str,
        cuerpo: Vec<u8>,
        que: &str,
    ) {
        let sesion = self.sesion(quien);
        let (estado, _, bytes) =
            pedir_bytes(&self.app, metodo.clone(), ruta, sesion.as_deref(), tipo, cuerpo).await;
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        self.revisar(que, estado, &json);
    }

    /// La plantilla tal cual y todas sus variantes hostiles.
    async fn plantilla(&mut self, p: &Plantilla, tokens: &[String]) -> StatusCode {
        let sesion = self.sesion(p.quien);
        let base = pedir(
            &self.app,
            p.metodo.clone(),
            &p.ruta,
            sesion.as_deref(),
            p.cuerpo.clone(),
        )
        .await;
        let que = format!("{} {}", p.metodo, p.ruta);
        self.revisar(&que, base.estado, &base.json);
        for (ruta, cambio) in variantes_de_ruta(&p.ruta, tokens)
            .into_iter()
            .chain(variantes_de_consulta(&p.ruta))
        {
            self.json(
                &p.metodo,
                &ruta,
                p.quien,
                p.cuerpo.clone(),
                &format!("{} {} [{cambio}]", p.metodo, p.ruta),
            )
            .await;
        }
        if let Some(cuerpo) = &p.cuerpo {
            let mut rutas = Vec::new();
            nodos(cuerpo, &mut Vec::new(), &mut rutas);
            for ruta in &rutas {
                for hostil in hostiles() {
                    let mutado = cambiado(cuerpo, ruta, Some(&hostil));
                    let que = format!(
                        "{} {} [{} = {}]",
                        p.metodo,
                        p.ruta,
                        describir(ruta),
                        recortar(&hostil.to_string())
                    );
                    self.json(&p.metodo, &p.ruta, p.quien, Some(mutado), &que).await;
                }
                let sin = cambiado(cuerpo, ruta, None);
                let que = format!("{} {} [sin {}]", p.metodo, p.ruta, describir(ruta));
                self.json(&p.metodo, &p.ruta, p.quien, Some(sin), &que).await;
            }
            // El cuerpo entero, roto.
            let anidado = format!("{}1{}", "[".repeat(300), "]".repeat(300));
            let rotos: Vec<(&str, Vec<u8>, &str)> = vec![
                ("application/json", b"{".to_vec(), "JSON mal formado"),
                ("application/json", Vec::new(), "cuerpo vacío"),
                ("application/json", b"[]".to_vec(), "arreglo"),
                ("application/json", b"\"texto\"".to_vec(), "texto"),
                ("application/json", b"null".to_vec(), "null"),
                ("application/json", anidado.into_bytes(), "anidado 300 niveles"),
                ("text/plain", cuerpo.to_string().into_bytes(), "text/plain"),
                ("", cuerpo.to_string().into_bytes(), "sin Content-Type"),
                ("application/json", b"\xff\xfe{}".to_vec(), "UTF-8 inválido"),
            ];
            for (tipo, bytes, que) in rotos {
                let que = format!("{} {} [{que}]", p.metodo, p.ruta);
                self.crudo(&p.metodo, &p.ruta, p.quien, tipo, bytes, &que).await;
            }
        }
        base.estado
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn ninguna_entrada_hostil_produce_un_error_del_servidor() {
    let base = BaseDePrueba::crear(&migraciones::origenes()).await;
    let raiz = std::env::temp_dir().join(format!("diamante-robustez-{}", std::process::id()));
    let raiz_texto = raiz.display().to_string();
    let configuracion = Configuracion::desde(|clave| match clave {
        "DATABASE_URL" => Some("postgres://sin-uso".into()),
        "DIAMANTE_URL_PUBLICA" => Some(SITIO.into()),
        "DIAMANTE_ARCHIVOS_DIR" => Some(raiz_texto.clone()),
        "DIAMANTE_LIMITES_DE_USO" => Some("false".into()),
        "DIAMANTE_WHATSAPP_SECRETO_APP" => Some(SECRETO_APP.into()),
        "DIAMANTE_WHATSAPP_TOKEN_VERIFICACION" => Some("token-de-verificacion".into()),
        _ => None,
    })
    .unwrap();
    let composicion =
        Composicion::con_pool(base.pool.clone(), Arc::new(nucleo::RelojSistema), &configuracion).unwrap();
    let app = enrutador::construir(&composicion);

    // Escenario: lo mínimo para que cada ruta llegue hasta el fondo con su cuerpo válido.
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
    let (_, admin) = entrar(&app, "laura@diamante.co").await;
    let crear = |ruta: String, cuerpo: Value| {
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
            assert!(r.estado.is_success(), "{ruta}: {} {}", r.estado, r.json);
            r.json
        }
    };
    let id = |v: &Value| v["id"].as_i64().unwrap();
    let hoy = chrono::Utc::now()
        .with_timezone(&nucleo::ZONA_HORARIA)
        .date_naive();
    let en = |dias: u64| hoy.checked_add_days(Days::new(dias)).unwrap();
    let dia = (8..15).map(en).find(|d| d.weekday() != Weekday::Sun).unwrap();
    let a_las = |hora: &str| format!("{dia}T{hora}:00-05:00");
    let mes = format!("{:04}-{:02}", hoy.year(), hoy.month());

    let admin2 = id(&crear(
        "/administradores".into(),
        json!({ "correo": "camila@diamante.co", "nombre": "Camila Rojas" }),
    )
    .await);
    let politica = id(&crear(
        "/negocio/politicas".into(),
        json!({ "tipo": "tratamiento_datos", "version": "1.0", "url": "https://diamante.co/politicas/datos" }),
    )
    .await);
    let servicio = |nombre: &str| json!({ "categoria_id": 1, "nombre": nombre, "descripcion": "Servicio.", "precio": 35_000, "duracion_min": 30 });
    let corte = id(&crear("/servicios".into(), servicio("Corte clásico")).await);
    let cejas = id(&crear("/servicios".into(), servicio("Diseño de cejas")).await);
    let otro = id(&crear("/servicios".into(), servicio("Otro servicio")).await);
    let combo = id(&crear(
        "/servicios".into(),
        json!({ "tipo": "combo", "categoria_id": 1, "nombre": "Corte y cejas", "descripcion": "Combo.", "precio": 40_000, "duracion_min": 50,
                "componentes": [corte, cejas] }),
    )
    .await);
    let santiago = id(&crear(
        "/barberos".into(),
        registro(
            "Santiago",
            "1020304050",
            "santiago@diamante.co",
            vec![json!(corte), json!(cejas)],
        ),
    )
    .await);
    let mateo = id(&crear(
        "/barberos".into(),
        registro(
            "Mateo",
            "1020304051",
            "mateo@diamante.co",
            vec![json!(corte), json!(cejas)],
        ),
    )
    .await);
    let daniel = id(&crear(
        "/barberos".into(),
        registro("Daniel", "1020304052", "daniel@diamante.co", vec![json!(corte)]),
    )
    .await);
    activar("santiago@diamante.co").await;
    let (_, barbero) = entrar(&app, "santiago@diamante.co").await;

    let cita_mateo = crear(
        "/agenda/citas".into(),
        json!({ "cliente_nuevo": { "nombre": "Pablo Vélez", "celular": "3105559917" }, "servicio_id": corte,
                "barbero_id": mateo, "inicio": a_las("10:00") }),
    )
    .await;
    let (cita, pablo) = (id(&cita_mateo), cita_mateo["cliente"]["id"].as_i64().unwrap());
    let cita2 = id(&crear(
        "/agenda/citas".into(),
        json!({ "cliente_nuevo": { "nombre": "Diana Ruiz", "celular": "3001234567" }, "servicio_id": corte,
                "barbero_id": santiago, "inicio": a_las("11:00") }),
    )
    .await);
    let libre = id(&crear(
        "/clientes".into(),
        json!({ "nombre": "Luis Mora", "celular": "3007654321" }),
    )
    .await);
    let solicitud = id(&crear(
        "/clientes/solicitudes-titular".into(),
        json!({ "cliente_id": libre, "tipo": "consulta", "descripcion": "Quiere saber qué datos hay." }),
    )
    .await);
    let bloqueo = id(&crear(
        format!("/barberos/{mateo}/bloqueos"),
        json!({ "fecha": dia, "desde": "15:00", "hasta": "16:00", "motivo": "otro" }),
    )
    .await);
    let ausencia = id(&crear(
        format!("/barberos/{mateo}/ausencias"),
        json!({ "tipo": "vacaciones", "desde": en(40), "hasta": en(41), "observacion": "Viaje." }),
    )
    .await);
    let dia_especial = id(&crear(
        "/negocio/dias-especiales".into(),
        json!({ "fecha": en(30), "nombre": "Cierre temprano", "tipo": "cierre_temprano", "hora_cierre": "14:00:00" }),
    )
    .await);
    let (tipo, cuerpo) = multipart(&[
        ("tipo", None, b"contrato"),
        ("archivo", Some("contrato.pdf"), b"%PDF-1.4\n%%EOF\n"),
    ]);
    let (estado, _, bytes) = pedir_bytes(
        &app,
        Method::POST,
        &format!("/api/v1/barberos/{mateo}/documentos"),
        Some(&admin),
        &tipo,
        cuerpo,
    )
    .await;
    assert!(
        estado.is_success(),
        "documento: {estado} {}",
        String::from_utf8_lossy(&bytes)
    );
    let documento = serde_json::from_slice::<Value>(&bytes).unwrap()["id"]
        .as_i64()
        .unwrap();
    let (estado, cabeceras, _) = pedir_bytes(
        &app,
        Method::GET,
        &format!("/api/v1/barberos/{mateo}/documentos/{documento}/descarga"),
        Some(&admin),
        "",
        Vec::new(),
    )
    .await;
    assert!(estado.is_redirection(), "descarga: {estado}");
    let firmada = cabeceras[header::LOCATION].to_str().unwrap().to_owned();

    let catalogos = pedir(&app, Method::GET, "/api/v1/ventas/catalogos", Some(&admin), None)
        .await
        .json;
    let metodo = |codigo: &str| {
        catalogos["metodos_pago"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["codigo"] == codigo)
            .map(|m| m["id"].as_i64().unwrap())
            .unwrap()
    };
    let (efectivo, nequi) = (metodo("efectivo"), metodo("nequi"));
    let descuento = catalogos["descuentos"][0]["id"].as_i64().unwrap();
    let motivo = catalogos["motivos_descuento"][0]["id"].as_i64().unwrap();
    let venta = |barbero_id: i64| {
        json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": barbero_id, "servicio_ids": [corte],
                                          "propina": 1000, "pagos": [{ "metodo_pago_id": efectivo, "monto": 36_000 }] })
    };
    let venta1 = id(&crear("/ventas".into(), venta(mateo)).await);
    let venta2 = id(&crear("/ventas".into(), venta(mateo)).await);

    // Reserva en línea: comprobantes para reservar y un enlace de gestión.
    let comprobante = |celular: &'static str| {
        let (app, base) = (app.clone(), &base);
        async move {
            verificar(&app, base, celular).await["comprobante"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    let reserva = |comprobante: String, hora: &str| {
        json!({ "comprobante": comprobante, "servicio_id": corte, "barbero_id": mateo, "inicio": a_las(hora),
                "cliente": { "nombre": "Ana Torres", "correo": null, "fecha_nacimiento": null },
                "autoriza_tratamiento_datos": true, "politica_version_id": politica })
    };
    let r = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/citas",
        None,
        Some(reserva(comprobante("3001112233").await, "12:00")),
    )
    .await;
    assert_eq!(r.estado, StatusCode::CREATED, "{}", r.json);
    let gestion = r.json["enlace_gestion"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    let comprobante_libre = comprobante("3002223344").await;
    let verificacion = pedir(
        &app,
        Method::POST,
        "/api/v1/publico/verificaciones",
        None,
        Some(json!({ "celular": "3003334455" })),
    )
    .await
    .json["id"]
        .as_i64()
        .unwrap();
    let negocio = pedir(&app, Method::GET, "/api/v1/negocio", Some(&admin), None)
        .await
        .json;
    let datos_negocio = json!({
        "nombre_comercial": negocio["nombre_comercial"], "nit": negocio["nit"], "direccion": negocio["direccion"],
        "zona_referencia": negocio["zona_referencia"], "whatsapp": negocio["whatsapp"], "instagram": negocio["instagram"],
        "porcentaje_comision_defecto": negocio["porcentaje_comision_defecto"],
    });
    let mut datos_barbero = registro("Mateo", "1020304051", "mateo@diamante.co", vec![]);
    for campo in ["servicios", "horario", "visibilidad"] {
        datos_barbero.as_object_mut().unwrap().remove(campo);
    }
    let anio = hoy.year() + 1;

    use Method as M;
    use Quien::{Admin, Barbero, Nadie};
    let plantillas = vec![
        // Sitio público y archivos.
        p(M::GET, "/publico/landing", None, Nadie),
        p(M::GET, "/publico/servicios", None, Nadie),
        p(
            M::GET,
            format!("/publico/barberos?servicio_id={corte}"),
            None,
            Nadie,
        ),
        p(
            M::GET,
            format!("/publico/disponibilidad/calendario?servicio_id={corte}&barbero_id={mateo}&mes={mes}"),
            None,
            Nadie,
        ),
        p(
            M::GET,
            format!("/publico/disponibilidad/horas?servicio_id={corte}&barbero_id={mateo}&fecha={dia}"),
            None,
            Nadie,
        ),
        p(
            M::POST,
            "/publico/verificaciones",
            Some(json!({ "celular": "3004445566" })),
            Nadie,
        ),
        p(
            M::POST,
            format!("/publico/verificaciones/{verificacion}/confirmar"),
            Some(json!({ "codigo": "0000" })),
            Nadie,
        ),
        p(
            M::POST,
            "/publico/citas",
            Some(reserva(comprobante_libre, "13:00")),
            Nadie,
        ),
        p(M::GET, format!("/publico/citas/gestion/{gestion}"), None, Nadie),
        p(
            M::PUT,
            format!("/publico/citas/gestion/{gestion}/recordatorios"),
            Some(json!({ "activos": false })),
            Nadie,
        ),
        p(
            M::POST,
            format!("/publico/citas/gestion/{gestion}/reprogramar"),
            Some(json!({ "inicio": a_las("16:30"), "barbero_id": mateo })),
            Nadie,
        ),
        p(M::GET, "/media/publico/no-existe.webp", None, Nadie),
        p(M::GET, "/media/publico/../../../etc/passwd", None, Nadie),
        p(M::GET, "/media/publico/%00.webp", None, Nadie),
        p(M::GET, "/media/publico/a%2F..%2F..%2Fsecreto", None, Nadie),
        p(M::GET, format!("/media/{}", "x".repeat(3_000)), None, Nadie),
        p(
            M::GET,
            firmada.trim_start_matches("/api/v1").to_owned(),
            None,
            Nadie,
        ),
        p(
            M::GET,
            "/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=token-de-verificacion&hub.challenge=123",
            None,
            Nadie,
        ),
        // Autenticación (con cuentas que no existen: nada se bloquea).
        p(
            M::POST,
            "/auth/sesion",
            Some(
                json!({ "correo": "nadie@diamante.co", "contrasena": "una frase cualquiera", "recordarme": true }),
            ),
            Nadie,
        ),
        p(
            M::POST,
            "/auth/recuperacion",
            Some(json!({ "correo": "nadie@diamante.co" })),
            Nadie,
        ),
        p(
            M::POST,
            "/auth/recuperacion/confirmar",
            Some(json!({ "token": "no-existe", "contrasena": "frase de prueba bien larga" })),
            Nadie,
        ),
        p(
            M::POST,
            "/auth/activacion",
            Some(json!({ "token": "no-existe", "contrasena": "frase de prueba bien larga" })),
            Nadie,
        ),
        p(M::GET, "/auth/sesion", None, Admin),
        p(
            M::PUT,
            "/auth/contrasena",
            Some(json!({ "actual": "no es la contraseña", "nueva": "otra frase bien larga" })),
            Admin,
        ),
        p(M::DELETE, "/auth/sesion", None, Nadie),
        // Lecturas del panel.
        p(M::GET, "/administradores", None, Admin),
        p(
            M::GET,
            format!(
                "/auditoria?entidad=barbero&entidad_id={mateo}&desde={hoy}&hasta={hoy}&pagina=1&por_pagina=20"
            ),
            None,
            Admin,
        ),
        p(
            M::GET,
            "/avisos?no_leidos=true&pagina=1&por_pagina=20",
            None,
            Admin,
        ),
        p(
            M::GET,
            "/mensajes?destinatario=3105559917&estado=pendiente&canal=whatsapp&plantilla=confirmacion_cita&pagina=1&por_pagina=20",
            None,
            Admin,
        ),
        p(M::GET, "/negocio", None, Admin),
        p(M::GET, "/negocio/horario-atencion", None, Admin),
        p(
            M::GET,
            format!("/negocio/dias-especiales?desde={hoy}&hasta={dia}"),
            None,
            Admin,
        ),
        p(M::GET, "/negocio/politicas", None, Admin),
        p(M::GET, "/categorias", None, Admin),
        p(M::GET, "/servicios?incluir_inactivos=true", None, Admin),
        p(M::GET, format!("/servicios/{corte}"), None, Admin),
        p(M::GET, "/barberos?q=San&estado=activo", None, Admin),
        p(M::GET, format!("/barberos/{mateo}"), None, Admin),
        p(M::GET, format!("/barberos/{mateo}/horario"), None, Admin),
        p(M::GET, format!("/barberos/{mateo}/servicios"), None, Admin),
        p(
            M::GET,
            format!("/barberos/{mateo}/bloqueos?desde={hoy}"),
            None,
            Admin,
        ),
        p(M::GET, format!("/barberos/{mateo}/ausencias"), None, Admin),
        p(M::GET, format!("/barberos/{mateo}/documentos"), None, Admin),
        p(M::GET, format!("/barberos/{mateo}/datos-privados"), None, Admin),
        p(
            M::GET,
            format!("/barberos/{mateo}/documentos/{documento}/descarga"),
            None,
            Admin,
        ),
        p(
            M::GET,
            "/clientes?q=Pa&segmento=nuevo&pagina=1&por_pagina=20",
            None,
            Admin,
        ),
        p(M::GET, "/clientes/buscar?q=Pab", None, Admin),
        p(M::GET, format!("/clientes/{pablo}"), None, Admin),
        p(
            M::GET,
            "/clientes/solicitudes-titular?abiertas=true&pagina=1&por_pagina=20",
            None,
            Admin,
        ),
        p(
            M::GET,
            format!("/agenda/citas?desde={dia}&hasta={dia}&barbero_id={mateo}&estado=sin_confirmar"),
            None,
            Admin,
        ),
        p(M::GET, format!("/agenda/citas/{cita}"), None, Admin),
        p(M::GET, format!("/agenda/tablero?fecha={dia}"), None, Admin),
        p(
            M::GET,
            format!("/agenda/disponibilidad/horas?servicio_id={corte}&barbero_id={mateo}&fecha={dia}"),
            None,
            Admin,
        ),
        p(
            M::GET,
            format!("/agenda/pendientes-de-cierre?fecha={hoy}"),
            None,
            Admin,
        ),
        p(M::GET, "/agenda/configuracion", None, Admin),
        p(
            M::GET,
            format!("/ventas?fecha={hoy}&barbero_id={mateo}&incluir_anuladas=true&pagina=1&por_pagina=20"),
            None,
            Admin,
        ),
        p(
            M::GET,
            format!("/ventas?desde={hoy}&hasta={hoy}&cliente_id={pablo}"),
            None,
            Admin,
        ),
        p(M::GET, format!("/ventas/{venta1}"), None, Admin),
        p(M::GET, format!("/ventas/caja?fecha={hoy}"), None, Admin),
        p(M::GET, "/ventas/catalogos", None, Admin),
        p(
            M::GET,
            format!("/ventas/resumen-barbero?fecha={hoy}&barbero_id={mateo}"),
            None,
            Admin,
        ),
        p(M::GET, format!("/ventas/cierres?fecha={hoy}"), None, Admin),
        p(M::GET, "/ventas/metodos-pago", None, Admin),
        p(M::GET, format!("/reportes/dashboard?fecha={hoy}"), None, Admin),
        p(M::GET, format!("/reportes/tendencias?mes={mes}"), None, Admin),
        p(M::GET, "/reportes/alertas", None, Admin),
        p(M::GET, format!("/reportes/resumen?mes={mes}"), None, Admin),
        p(
            M::GET,
            format!("/reportes/resumen?desde={hoy}&hasta={hoy}"),
            None,
            Admin,
        ),
        p(M::GET, format!("/reportes/resumen.pdf?mes={mes}"), None, Admin),
        p(
            M::GET,
            format!("/reportes/barberos/{mateo}?mes={mes}"),
            None,
            Admin,
        ),
        p(
            M::GET,
            format!("/reportes/barberos/{mateo}.pdf?mes={mes}"),
            None,
            Admin,
        ),
        p(M::GET, format!("/reportes/barberos.pdf?mes={mes}"), None, Admin),
        p(M::GET, format!("/reportes/clientes?mes={mes}"), None, Admin),
        p(M::GET, format!("/reportes/clientes.pdf?mes={mes}"), None, Admin),
        p(M::GET, "/yo", None, Barbero),
        p(
            M::GET,
            format!("/ventas/resumen-barbero?fecha={hoy}"),
            None,
            Barbero,
        ),
        p(
            M::GET,
            format!("/reportes/barberos/{santiago}?mes={mes}"),
            None,
            Barbero,
        ),
        // El orden va antes de las altas: debe nombrar a todo el equipo vigente.
        p(
            M::PUT,
            "/barberos/orden",
            Some(json!({ "barbero_ids": [santiago, mateo, daniel] })),
            Admin,
        ),
        // Altas.
        p(
            M::POST,
            "/administradores",
            Some(json!({ "correo": "otra@diamante.co", "nombre": "Otra Persona" })),
            Admin,
        ),
        p(
            M::POST,
            "/negocio/dias-especiales",
            Some(
                json!({ "fecha": en(50), "nombre": "Cierre", "tipo": "cierre_temprano", "hora_cierre": "14:00:00" }),
            ),
            Admin,
        ),
        p(
            M::POST,
            format!("/negocio/dias-especiales/festivos/{anio}"),
            None,
            Admin,
        ),
        p(
            M::POST,
            "/negocio/politicas",
            Some(
                json!({ "tipo": "tratamiento_datos", "version": "2.0", "url": "https://diamante.co/politicas/datos-v2" }),
            ),
            Admin,
        ),
        p(M::POST, "/servicios", Some(servicio("Servicio de prueba")), Admin),
        p(
            M::POST,
            "/servicios",
            Some(
                json!({ "tipo": "combo", "categoria_id": 1, "nombre": "Combo de prueba", "descripcion": "Combo.",
                                              "precio": 40_000, "duracion_min": 50, "componentes": [corte, cejas], "etiqueta": "Nuevo", "destacado": true, "orden": 2 }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/barberos",
            Some(registro(
                "Nuevo",
                "1020304099",
                "nuevo@diamante.co",
                vec![json!(corte)],
            )),
            Admin,
        ),
        p(
            M::POST,
            format!("/barberos/{mateo}/bloqueos"),
            Some(
                json!({ "fecha": dia, "desde": "17:00", "hasta": "18:00", "motivo": "cita_medica", "nota": "Odontólogo" }),
            ),
            Admin,
        ),
        p(
            M::POST,
            format!("/barberos/{mateo}/ausencias"),
            Some(json!({ "tipo": "incapacidad", "desde": en(60), "hasta": en(61), "observacion": "Gripa." })),
            Admin,
        ),
        p(
            M::POST,
            "/clientes",
            Some(
                json!({ "nombre": "Cliente Nuevo", "celular": "3009998877", "correo": "nuevo@correo.co", "fecha_nacimiento": "1990-05-12" }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/clientes/solicitudes-titular",
            Some(json!({ "cliente_id": pablo, "tipo": "actualizacion", "descripcion": "Cambió de correo." })),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/citas",
            Some(
                json!({ "cliente_nuevo": { "nombre": "Carlos Pérez", "celular": "3108887766", "correo": "carlos@correo.co", "fecha_nacimiento": "1988-01-02" },
                                                 "servicio_id": corte, "barbero_id": mateo, "inicio": a_las("14:00"), "nota": "Llamó por teléfono." }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/citas",
            Some(
                json!({ "cliente_id": pablo, "servicio_id": cejas, "barbero_id": santiago, "inicio": a_las("14:30") }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/conflictos",
            Some(json!({ "barbero_id": mateo, "ausencia": { "desde": dia, "hasta": dia } })),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/conflictos",
            Some(
                json!({ "barbero_id": mateo, "bloqueo": { "fecha": dia, "desde": "09:00", "hasta": "12:00" } }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/conflictos",
            Some(
                json!({ "barbero_id": mateo, "horario_propuesto": [{ "dia_semana": 1, "inicio": "09:00", "fin": "12:00" }] }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/ventas",
            Some(
                json!({ "cliente": { "tipo": "nuevo", "nombre": "Venta Nueva", "celular": "3017776655", "correo": null, "fecha_nacimiento": null },
                                           "barbero_id": mateo, "servicio_ids": [corte, cejas], "descuento": { "descuento_id": descuento, "motivo_descuento_id": motivo },
                                           "propina": 2000, "pagos": [{ "metodo_pago_id": efectivo, "monto": 40_000 }, { "metodo_pago_id": nequi, "monto": 25_000 }] }),
            ),
            Admin,
        ),
        p(
            M::POST,
            "/ventas",
            Some(
                json!({ "cliente": { "tipo": "registrado", "id": pablo }, "barbero_id": mateo, "servicio_ids": [corte],
                                           "propina": 0, "pagos": [{ "metodo_pago_id": efectivo, "monto": 35_000 }] }),
            ),
            Admin,
        ),
        p(M::POST, "/ventas", Some(venta(santiago)), Barbero),
        // Dos pagos al tope: la suma se compara sin desbordarse.
        p(
            M::POST,
            "/ventas",
            Some(
                json!({ "cliente": { "tipo": "ocasional" }, "barbero_id": mateo, "servicio_ids": [corte], "propina": 0,
                                           "pagos": [{ "metodo_pago_id": efectivo, "monto": nucleo::Dinero::MAXIMO },
                                                     { "metodo_pago_id": nequi, "monto": nucleo::Dinero::MAXIMO }] }),
            ),
            Admin,
        ),
        // Cambios.
        p(M::PUT, "/negocio", Some(datos_negocio), Admin),
        p(
            M::PUT,
            format!("/negocio/dias-especiales/{dia_especial}"),
            Some(
                json!({ "fecha": en(30), "nombre": "Cierre temprano", "tipo": "cierre_temprano", "hora_cierre": "15:00:00" }),
            ),
            Admin,
        ),
        p(
            M::PUT,
            format!("/servicios/{otro}"),
            Some(
                json!({ "categoria_id": 1, "nombre": "Otro servicio", "descripcion": "Otro.", "precio": 38_000, "duracion_min": 45,
                                                              "agendable_en_linea": true, "visible_en_landing": false }),
            ),
            Admin,
        ),
        p(
            M::PATCH,
            format!("/servicios/{otro}/estado"),
            Some(json!({ "activo": true })),
            Admin,
        ),
        p(
            M::PUT,
            format!("/servicios/{combo}/componentes"),
            Some(json!({ "servicio_ids": [corte, cejas] })),
            Admin,
        ),
        p(M::PUT, format!("/barberos/{mateo}"), Some(datos_barbero), Admin),
        p(
            M::PUT,
            format!("/barberos/{mateo}/horario"),
            Some(
                json!({ "franjas": [{ "dia_semana": 1, "inicio": "09:00", "fin": "19:00" },
                                                                                 { "dia_semana": 6, "inicio": "08:00", "fin": "14:00" }] }),
            ),
            Admin,
        ),
        p(
            M::PUT,
            format!("/barberos/{santiago}/horario"),
            Some(
                json!({ "franjas": (1..=6).map(|d| json!({ "dia_semana": d, "inicio": "09:00", "fin": "19:00" })).collect::<Vec<_>>() }),
            ),
            Barbero,
        ),
        p(
            M::PUT,
            format!("/barberos/{mateo}/servicios"),
            Some(json!({ "servicio_ids": [corte, cejas] })),
            Admin,
        ),
        p(
            M::PUT,
            format!("/barberos/{mateo}/visibilidad"),
            Some(json!({ "mostrar_en_landing": true, "recibe_citas_en_linea": true, "acceso_panel": false })),
            Admin,
        ),
        p(
            M::PUT,
            format!("/barberos/{mateo}/datos-privados"),
            Some(
                json!({ "direccion": "Cra. 15 # 93-47", "ciudad": "Bogotá", "grupo_sanguineo": "O+",
                                                                               "emergencia": { "nombre": "Marta Ruiz", "parentesco": "madre_padre", "celular": "3001112233" },
                                                                               "eps": "Sanitas", "medio_pago": "nequi", "numero_cuenta": "3001112233" }),
            ),
            Admin,
        ),
        p(
            M::PUT,
            format!("/clientes/{pablo}"),
            Some(
                json!({ "nombre": "Pablo Vélez", "celular": "3105559917", "correo": "pablo@correo.co",
                                                              "fecha_nacimiento": "1990-01-01", "preferencias": "Tijera.", "recordatorios_activos": true }),
            ),
            Admin,
        ),
        p(
            M::PATCH,
            format!("/clientes/solicitudes-titular/{solicitud}"),
            Some(json!({ "respuesta": "Se le enviaron sus datos." })),
            Admin,
        ),
        p(
            M::PUT,
            "/agenda/configuracion",
            Some(
                json!({ "anticipacion_minima_min": 120, "ventana_reserva_dias": 30, "intervalo_min": 30,
            "aviso_cancelacion_min": 180, "recordatorio_24h": true, "recordatorio_2h": true,
            "politica_inasistencias": "confirmar_por_llamada", "umbral_inasistencias": 2 }),
            ),
            Admin,
        ),
        p(
            M::PATCH,
            format!("/ventas/metodos-pago/{nequi}"),
            Some(json!({ "activo": true })),
            Admin,
        ),
        p(
            M::PATCH,
            format!("/administradores/{admin2}"),
            Some(json!({ "activo": false })),
            Admin,
        ),
        p(M::POST, "/avisos/1/leido", None, Admin),
        // Transiciones y acciones que no se deshacen.
        p(M::POST, format!("/agenda/citas/{cita}/confirmar"), None, Admin),
        p(M::POST, format!("/agenda/citas/{cita}/recordatorio"), None, Admin),
        p(M::POST, format!("/agenda/citas/{cita}/iniciar"), None, Admin),
        p(M::POST, format!("/agenda/citas/{cita}/inasistencia"), None, Admin),
        p(
            M::POST,
            format!("/agenda/citas/{cita}/reprogramar"),
            Some(json!({ "inicio": a_las("16:00"), "barbero_id": mateo })),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/citas/reasignacion",
            Some(json!({ "cita_ids": [cita], "barbero_destino_id": santiago })),
            Admin,
        ),
        p(
            M::POST,
            "/agenda/citas/aviso-reprogramacion",
            Some(json!({ "cita_ids": [cita2] })),
            Admin,
        ),
        p(
            M::POST,
            format!("/agenda/citas/{cita2}/cancelar"),
            Some(json!({ "motivo": "solicitud_cliente", "avisar_cliente": true })),
            Admin,
        ),
        p(
            M::POST,
            format!("/publico/citas/gestion/{gestion}/cancelar"),
            None,
            Nadie,
        ),
        p(
            M::POST,
            format!("/barberos/{mateo}/ausencias/{ausencia}/anular"),
            None,
            Admin,
        ),
        p(
            M::DELETE,
            format!("/barberos/{mateo}/bloqueos/{bloqueo}"),
            None,
            Admin,
        ),
        p(
            M::DELETE,
            format!("/barberos/{mateo}/documentos/{documento}"),
            None,
            Admin,
        ),
        p(
            M::DELETE,
            format!("/negocio/dias-especiales/{dia_especial}"),
            None,
            Admin,
        ),
        p(
            M::POST,
            format!("/ventas/{venta2}/anulacion"),
            Some(json!({ "motivo": "Me equivoqué de servicio" })),
            Admin,
        ),
        // Dos conteos al tope de `Dinero` (el cierre queda con ellos y se vuelve a leer): la
        // diferencia total no se desborda.
        p(
            M::POST,
            "/ventas/cierres",
            Some(
                json!({ "fecha": hoy, "contado": [{ "metodo_pago_id": efectivo, "monto": nucleo::Dinero::MAXIMO },
                                                                           { "metodo_pago_id": nequi, "monto": nucleo::Dinero::MAXIMO }] }),
            ),
            Admin,
        ),
        p(M::GET, format!("/ventas/cierres?fecha={hoy}"), None, Admin),
        p(
            M::POST,
            "/ventas/cierres",
            Some(
                json!({ "fecha": hoy, "contado": [{ "metodo_pago_id": efectivo, "monto": 100_000 }, { "metodo_pago_id": nequi, "monto": 0 }],
                                                   "observacion": "Conteo de prueba" }),
            ),
            Admin,
        ),
        p(
            M::POST,
            format!("/clientes/{libre}/anonimizacion"),
            Some(json!({ "motivo": "Solicitud de supresión del titular" })),
            Admin,
        ),
        p(
            M::POST,
            format!("/barberos/{daniel}/retiro"),
            Some(json!({ "fecha": hoy, "motivo": "renuncia_voluntaria" })),
            Admin,
        ),
    ];

    let mut fuzz = Fuzz {
        app: app.clone(),
        admin,
        barbero,
        fallas: Vec::new(),
        peticiones: 0,
    };
    let tokens = [gestion.clone()];
    let mut bases = Vec::new();
    for plantilla in &plantillas {
        let estado = fuzz.plantilla(plantilla, &tokens).await;
        // Sin mutar, ninguna plantilla debe fallar por su forma (400), su sesión (401/403, salvo
        // el inicio de sesión con una cuenta inventada) o su método (405): si no, las mutaciones
        // de esa ruta no llegarían hasta el fondo.
        let login = plantilla.ruta.ends_with("/auth/sesion") && plantilla.metodo == Method::POST;
        if matches!(estado.as_u16(), 400 | 403 | 405) || (estado == StatusCode::UNAUTHORIZED && !login) {
            bases.push(format!("{estado} {} {}", plantilla.metodo, plantilla.ruta));
        }
    }

    // Webhook de Meta firmado: cada nodo del cuerpo cambiado, con la firma correcta.
    let respuesta = json!({ "object": "whatsapp_business_account", "entry": [{ "id": "WABA", "changes": [{ "field": "messages",
        "value": { "messaging_product": "whatsapp",
                   "statuses": [{ "id": "wamid.no-existe", "status": "delivered", "timestamp": "1763500000", "recipient_id": "573001112233" }],
                   "messages": [{ "from": "573001112233", "id": "wamid.respuesta", "timestamp": "1763500000", "type": "button",
                                  "context": { "from": "573000000000", "id": "wamid.no-existe" },
                                  "button": { "payload": "confirmar", "text": "Confirmo" } }] } }] }] });
    let mut rutas = Vec::new();
    nodos(&respuesta, &mut Vec::new(), &mut rutas);
    for ruta in &rutas {
        for hostil in hostiles().into_iter().map(Some).chain([None]) {
            let cuerpo = cambiado(&respuesta, ruta, hostil.as_ref());
            let r = webhook(&app, &cuerpo, firmado(&cuerpo)).await;
            let que = format!(
                "POST /webhooks/whatsapp [{} = {}]",
                describir(ruta),
                hostil.map_or("(sin)".into(), |h| recortar(&h.to_string()))
            );
            fuzz.revisar(&que, r.estado, &r.json);
        }
    }

    // El worker procesa todos esos eventos raros sin pánico y sin quedarse con ninguno.
    let mut procesados = 0;
    loop {
        let resumen = composicion.notificaciones.procesar_webhooks().await.unwrap();
        if resumen.eventos == 0 {
            break;
        }
        procesados += resumen.eventos;
    }
    assert!(
        procesados >= rutas.len() * hostiles().len(),
        "procesó {procesados} eventos"
    );

    // Formularios con archivos: textos y nombres de archivo hostiles.
    let pdf = b"%PDF-1.4\n%%EOF\n".to_vec();
    let foto = png(800, 1000);
    let nombre_enorme = "x".repeat(3_000);
    let formularios: Vec<FormularioHostil> = vec![
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("foto", Some("foto.png"), &foto)],
            "foto válida",
        ),
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("foto", Some("fo\0to.png"), &foto)],
            "nombre con nulo",
        ),
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("foto", Some("foto.png"), b"no es imagen")],
            "no es imagen",
        ),
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("foto", Some("foto.png"), b"")],
            "vacía",
        ),
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("otra", Some("foto.png"), &foto)],
            "campo equivocado",
        ),
        (
            format!("/api/v1/barberos/{mateo}/foto"),
            vec![("foto", None, b"texto")],
            "sin archivo",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![("tipo", None, b"con\0trato"), ("archivo", Some("c.pdf"), &pdf)],
            "tipo con nulo",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![("tipo", None, b"inventado"), ("archivo", Some("c.pdf"), &pdf)],
            "tipo inventado",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![
                ("tipo", None, b"pila"),
                ("periodo", None, b"+262142-12"),
                ("archivo", Some("c.pdf"), &pdf),
            ],
            "periodo de otro milenio",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![
                ("tipo", None, b"pila"),
                ("periodo", None, b"2026-13"),
                ("archivo", Some("c.pdf"), &pdf),
            ],
            "periodo inválido",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![
                ("tipo", None, b"contrato"),
                ("archivo", Some(&nombre_enorme), &pdf),
            ],
            "nombre enorme",
        ),
        (
            format!("/api/v1/barberos/{mateo}/documentos"),
            vec![("tipo", None, b"contrato")],
            "sin archivo",
        ),
        (
            format!("/api/v1/barberos/{mateo}/ausencias/{ausencia}/soporte"),
            vec![("soporte", Some("s.pdf"), &pdf)],
            "soporte de una ausencia anulada",
        ),
        (
            format!("/api/v1/barberos/{mateo}/ausencias/99999/soporte"),
            vec![("soporte", Some("s.pdf"), &pdf)],
            "ausencia inexistente",
        ),
    ];
    for (ruta, partes, que) in formularios {
        let (tipo, cuerpo) = multipart(&partes);
        fuzz.crudo(
            &Method::POST,
            &ruta,
            Quien::Admin,
            &tipo,
            cuerpo,
            &format!("POST {ruta} [{que}]"),
        )
        .await;
    }
    for ruta in [
        format!("/api/v1/barberos/{mateo}/foto"),
        format!("/api/v1/barberos/{mateo}/documentos"),
    ] {
        fuzz.crudo(
            &Method::POST,
            &ruta,
            Quien::Admin,
            "multipart/form-data; boundary=x",
            b"--x\r\nbasura".to_vec(),
            &format!("POST {ruta} [multipart roto]"),
        )
        .await;
        fuzz.crudo(
            &Method::POST,
            &ruta,
            Quien::Admin,
            "application/json",
            b"{}".to_vec(),
            &format!("POST {ruta} [JSON en lugar de formulario]"),
        )
        .await;
    }

    assert!(bases.is_empty(), "plantillas mal armadas:\n{}", bases.join("\n"));
    assert!(
        fuzz.fallas.is_empty(),
        "{} de {} peticiones fallaron:\n{}",
        fuzz.fallas.len(),
        fuzz.peticiones,
        fuzz.fallas
            .iter()
            .take(80)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    let _ = std::fs::remove_dir_all(&raiz);
    base.eliminar().await;
}
