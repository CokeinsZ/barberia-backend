//! Datos de prueba para preproducción: `diamante datos-de-prueba`.
//!
//! Llena la base con una barbería en marcha para que el equipo del frontend vea pantallas con
//! información real: servicios y combos, cinco barberos con su horario, unos 170 clientes de
//! todos los segmentos y 12 semanas de historia (citas en línea y del panel, cobros con
//! descuentos, propinas y pagos divididos, inasistencias, cancelaciones, ventas anuladas y
//! vueltas a cobrar, cierres de caja), el día de hoy según la hora y 3 semanas de citas por venir
//! (con una vacación que deja citas afectadas y su aviso, y un bloqueo).
//!
//! Cómo: arma la aplicación completa en este proceso con un reloj propio ([`RelojDePruebas`]) y
//! la recorre por su API (`/api/v1`, sin red), moviendo el reloj a cada momento: así todo pasa
//! por las mismas reglas, eventos y proyecciones que en el uso real (segmentos, visitas, caja,
//! reportes, auditoría). Actúa con una administradora propia (`datos-de-prueba@example.com`)
//! que se desactiva al terminar.
//!
//! Seguros:
//! - Los clientes tienen celulares inventados (`320 999 0001` en adelante). Si los WhatsApp
//!   salen por Meta, el comando exige `DIAMANTE_WHATSAPP_SOLO_A` (que no los incluya): así
//!   ni estos datos ni lo que el equipo haga con ellos le escriben a nadie de verdad.
//! - No corre dos veces sobre la misma base (busca el primer celular de prueba).

#![allow(
    clippy::print_stdout,
    reason = "es la salida del comando para la persona que lo ejecuta"
)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::{DateTime, Datelike, Days, FixedOffset, NaiveDate, TimeDelta, TimeZone, Utc};
use nucleo::{Actor, Reloj, RelojDePruebas};
use plataforma::base_datos::conectar;
use plataforma::configuracion::Configuracion;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::ErrorDeArranque;
use crate::composicion::Composicion;
use crate::enrutador;

const CORREO_ADMIN: &str = "datos-de-prueba@example.com";
/// Celulares de los clientes de prueba: `3209990001`, `3209990002`…
const PRIMER_CELULAR: u64 = 3_209_990_001;
const CLIENTES: u64 = 170;
const DIAS_DE_HISTORIA: u64 = 84;
const DIAS_POR_VENIR: u64 = 21;
/// Una de cada tantas citas se reserva en línea (con código de verificación).
const PROBABILIDAD_EN_LINEA: f64 = 0.25;

type Resultado<T> = Result<T, ErrorDeArranque>;

fn error(mensaje: impl Into<String>) -> ErrorDeArranque {
    mensaje.into().into()
}

// ---------------------------------------------------------------------------------------------
// Azar reproducible (xorshift64*): la misma semilla da los mismos datos.
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Entero en `[desde, hasta]`.
    fn entre(&mut self, desde: u64, hasta: u64) -> u64 {
        desde + self.siguiente() % (hasta - desde + 1)
    }

    fn prob(&mut self, p: f64) -> bool {
        #[allow(clippy::cast_precision_loss, reason = "solo se compara con una probabilidad")]
        let x = (self.siguiente() >> 11) as f64 / (1_u64 << 53) as f64;
        x < p
    }

    fn indice(&mut self, largo: usize) -> usize {
        #[allow(clippy::cast_possible_truncation, reason = "los largos son pequeños")]
        let i = (self.siguiente() % largo as u64) as usize;
        i
    }

    /// Índice según pesos.
    fn ponderado(&mut self, pesos: &[u32]) -> usize {
        let total: u64 = pesos.iter().map(|p| u64::from(*p)).sum();
        let mut x = self.siguiente() % total.max(1);
        for (i, p) in pesos.iter().enumerate() {
            if x < u64::from(*p) {
                return i;
            }
            x -= u64::from(*p);
        }
        0
    }
}

// ---------------------------------------------------------------------------------------------
// Catálogo, equipo y clientes.
// ---------------------------------------------------------------------------------------------

struct DefServicio {
    nombre: &'static str,
    categoria: i64,
    precio: i64,
    /// Minutos; en un combo, la suma de sus componentes.
    duracion: u32,
    descripcion: &'static str,
    /// Índices de sus componentes (solo combos).
    componentes: &'static [usize],
    etiqueta: Option<&'static str>,
    /// Qué tanto se pide como servicio principal.
    peso: u32,
}

const SERVICIOS: [DefServicio; 10] = [
    DefServicio {
        nombre: "Corte clásico",
        categoria: 1,
        precio: 35_000,
        duracion: 45,
        descripcion: "Corte a tijera y máquina, con lavado y peinado.",
        componentes: &[],
        etiqueta: Some("El más pedido"),
        peso: 30,
    },
    DefServicio {
        nombre: "Fade o degradado",
        categoria: 1,
        precio: 40_000,
        duracion: 60,
        descripcion: "Degradado a máquina con acabado a navaja.",
        componentes: &[],
        etiqueta: None,
        peso: 20,
    },
    DefServicio {
        nombre: "Corte infantil",
        categoria: 1,
        precio: 28_000,
        duracion: 30,
        descripcion: "Para niños de hasta 12 años.",
        componentes: &[],
        etiqueta: None,
        peso: 6,
    },
    DefServicio {
        nombre: "Arreglo de barba",
        categoria: 2,
        precio: 25_000,
        duracion: 30,
        descripcion: "Perfilado con navaja y toalla caliente.",
        componentes: &[],
        etiqueta: None,
        peso: 10,
    },
    DefServicio {
        nombre: "Afeitado clásico",
        categoria: 2,
        precio: 30_000,
        duracion: 45,
        descripcion: "Afeitado completo con toalla caliente y aceites.",
        componentes: &[],
        etiqueta: None,
        peso: 5,
    },
    DefServicio {
        nombre: "Diseño o líneas",
        categoria: 4,
        precio: 10_000,
        duracion: 15,
        descripcion: "Líneas o figuras a navaja.",
        componentes: &[],
        etiqueta: None,
        peso: 0,
    },
    DefServicio {
        nombre: "Cejas",
        categoria: 4,
        precio: 8_000,
        duracion: 15,
        descripcion: "Perfilado de cejas a navaja.",
        componentes: &[],
        etiqueta: None,
        peso: 0,
    },
    DefServicio {
        nombre: "Mascarilla negra",
        categoria: 4,
        precio: 20_000,
        duracion: 30,
        descripcion: "Limpieza facial con mascarilla de carbón.",
        componentes: &[],
        etiqueta: None,
        peso: 2,
    },
    DefServicio {
        nombre: "Combo cabello + barba",
        categoria: 3,
        precio: 55_000,
        duracion: 75,
        descripcion: "Corte clásico y arreglo de barba.",
        componentes: &[0, 3],
        etiqueta: Some("Ahorras $5.000"),
        peso: 18,
    },
    DefServicio {
        nombre: "Combo completo",
        categoria: 3,
        precio: 65_000,
        duracion: 90,
        descripcion: "Fade, arreglo de barba y cejas.",
        componentes: &[1, 3, 6],
        etiqueta: None,
        peso: 7,
    },
];

/// Complementos que a veces se suman al cobrar.
const EXTRAS: [usize; 3] = [5, 6, 7];

struct DefBarbero {
    nombres: &'static str,
    apellidos: &'static str,
    especialidad: &'static str,
    biografia: &'static str,
    anios: i64,
    comision: f64,
    /// Días desde que entró (los de menos de 84 no tienen toda la historia).
    dias_en_el_equipo: u64,
    /// `(día 1 = lunes … 7 = domingo, inicio, fin)`.
    franjas: &'static [(u8, &'static str, &'static str)],
    servicios: &'static [usize],
    acceso_panel: bool,
}

const TODOS: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

const BARBEROS: [DefBarbero; 5] = [
    DefBarbero {
        nombres: "Santiago",
        apellidos: "Ríos",
        especialidad: "Fades y diseños",
        anios: 8,
        comision: 50.0,
        biografia: "Ocho años perfeccionando degradados y diseños a navaja.",
        dias_en_el_equipo: 900,
        franjas: &[
            (2, "09:00", "13:00"),
            (2, "14:00", "19:00"),
            (3, "09:00", "13:00"),
            (3, "14:00", "19:00"),
            (4, "09:00", "13:00"),
            (4, "14:00", "19:00"),
            (5, "09:00", "13:00"),
            (5, "14:00", "19:00"),
            (6, "09:00", "13:00"),
            (6, "14:00", "19:00"),
            (7, "10:00", "14:00"),
        ],
        servicios: TODOS,
        acceso_panel: true,
    },
    DefBarbero {
        nombres: "Mateo",
        apellidos: "Díaz",
        especialidad: "Cortes clásicos",
        anios: 12,
        comision: 50.0,
        biografia: "Tijera, navaja y conversación: el corte de siempre, bien hecho.",
        dias_en_el_equipo: 1500,
        franjas: &[
            (1, "10:00", "14:00"),
            (1, "15:00", "20:00"),
            (2, "10:00", "14:00"),
            (2, "15:00", "20:00"),
            (3, "10:00", "14:00"),
            (3, "15:00", "20:00"),
            (4, "10:00", "14:00"),
            (4, "15:00", "20:00"),
            (5, "10:00", "14:00"),
            (5, "15:00", "20:00"),
            (6, "09:00", "17:00"),
        ],
        servicios: TODOS,
        acceso_panel: false,
    },
    DefBarbero {
        nombres: "Felipe",
        apellidos: "Rojas",
        especialidad: "Barba y afeitado",
        anios: 6,
        comision: 45.0,
        biografia: "Especialista en barba: perfilado, afeitado clásico y toalla caliente.",
        dias_en_el_equipo: 600,
        franjas: &[
            (1, "09:00", "13:00"),
            (1, "14:00", "18:00"),
            (2, "09:00", "13:00"),
            (2, "14:00", "18:00"),
            (3, "09:00", "13:00"),
            (3, "14:00", "18:00"),
            (4, "09:00", "13:00"),
            (4, "14:00", "18:00"),
            (5, "09:00", "13:00"),
            (5, "14:00", "18:00"),
            (6, "09:00", "13:00"),
            (6, "14:00", "18:00"),
        ],
        servicios: TODOS,
        acceso_panel: false,
    },
    DefBarbero {
        nombres: "Daniel",
        apellidos: "Castro",
        especialidad: "Tendencias y color",
        anios: 5,
        comision: 45.0,
        biografia: "Lo último en cortes, texturas y diseños.",
        dias_en_el_equipo: 400,
        franjas: &[
            (3, "11:00", "15:00"),
            (3, "16:00", "20:00"),
            (4, "11:00", "15:00"),
            (4, "16:00", "20:00"),
            (5, "11:00", "15:00"),
            (5, "16:00", "20:00"),
            (6, "11:00", "15:00"),
            (6, "16:00", "20:00"),
            (7, "10:00", "15:00"),
        ],
        servicios: TODOS,
        acceso_panel: false,
    },
    DefBarbero {
        nombres: "Andrés",
        apellidos: "Mejía",
        especialidad: "Cortes infantiles",
        anios: 2,
        comision: 40.0,
        biografia: "Paciencia y buen pulso: el favorito de los más pequeños.",
        dias_en_el_equipo: 40,
        franjas: &[
            (2, "09:00", "17:00"),
            (3, "09:00", "17:00"),
            (4, "09:00", "17:00"),
            (5, "09:00", "17:00"),
            (6, "09:00", "17:00"),
        ],
        servicios: &[0, 1, 2, 3, 5, 6, 8],
        acceso_panel: false,
    },
];

const NOMBRES: [&str; 48] = [
    "Andrés",
    "Juan Pablo",
    "Carlos",
    "Sebastián",
    "Julián",
    "Camilo",
    "Alejandro",
    "David",
    "Esteban",
    "Nicolás",
    "Miguel Ángel",
    "Jorge",
    "Luis Fernando",
    "Diego",
    "Óscar",
    "Mauricio",
    "Ricardo",
    "Fabián",
    "Iván",
    "Hernán",
    "Samuel",
    "Tomás",
    "Emiliano",
    "Martín",
    "Simón",
    "Jerónimo",
    "Juan José",
    "Gabriel",
    "Felipe",
    "Cristian",
    "Brayan",
    "Kevin",
    "Jhon",
    "Wilson",
    "Fredy",
    "Álvaro",
    "Gustavo",
    "Héctor",
    "Rafael",
    "Manuel",
    "Valentina",
    "Daniela",
    "Laura",
    "Mariana",
    "Paula",
    "Natalia",
    "Sofía",
    "Camila",
];

const APELLIDOS: [&str; 40] = [
    "Gómez",
    "Rodríguez",
    "Martínez",
    "López",
    "García",
    "Hernández",
    "Pérez",
    "Sánchez",
    "Ramírez",
    "Torres",
    "Díaz",
    "Vargas",
    "Moreno",
    "Jiménez",
    "Rojas",
    "Castro",
    "Ortiz",
    "Rubio",
    "Muñoz",
    "Suárez",
    "Restrepo",
    "Cárdenas",
    "Ospina",
    "Zapata",
    "Arango",
    "Quintero",
    "Valencia",
    "Mejía",
    "Salazar",
    "Guerrero",
    "Pineda",
    "Londoño",
    "Montoya",
    "Cifuentes",
    "Bermúdez",
    "Echeverry",
    "Gallego",
    "Patiño",
    "Rincón",
    "Duarte",
];

const PREFERENCIAS: [&str; 8] = [
    "Prefiere máquina 2 a los lados.",
    "Degradado bajo, sin marcar mucho la línea.",
    "Barba corta y bien perfilada.",
    "No le gusta la cera; solo polvo texturizador.",
    "Pide toalla caliente en la barba.",
    "Viene con su hijo los sábados.",
    "Le gusta el corte con tijera arriba.",
    "Piel sensible: usar el aceite sin fragancia.",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perfil {
    Frecuente,
    Recurrente,
    Nuevo,
    Inactivo,
    SinVisitas,
}

#[derive(Debug)]
struct Cliente {
    id: Option<i64>,
    nombre: String,
    celular: String,
    correo: Option<String>,
    nacimiento: Option<NaiveDate>,
    preferencias: Option<&'static str>,
    servicio: usize,
    barbero: usize,
    /// Su primera visita la reserva en línea (así se registra, con su autorización).
    alta_en_linea: bool,
    /// Reservas en línea que le quedan (el límite de códigos por celular es por hora real).
    en_linea: u8,
}

/// Una cita planeada.
#[derive(Debug, Clone)]
struct Visita {
    cliente: usize,
    barbero: usize,
    servicio: usize,
    fecha: NaiveDate,
    /// Minutos desde la medianoche.
    inicio: u32,
}

fn sin_tildes(texto: &str) -> String {
    texto
        .chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' => 'u',
            'ñ' | 'Ñ' => 'n',
            ' ' => '.',
            otro => otro.to_ascii_lowercase(),
        })
        .collect()
}

fn minutos(hora: &str) -> u32 {
    let (h, m) = hora.split_once(':').unwrap_or(("0", "0"));
    h.parse::<u32>().unwrap_or(0) * 60 + m.parse::<u32>().unwrap_or(0)
}

fn bogota() -> FixedOffset {
    // Colombia no tiene horario de verano: siempre UTC−5.
    FixedOffset::west_opt(5 * 3600).expect("UTC−5 es un desfase válido")
}

/// El instante de `fecha` a esa hora (minutos) en Bogotá.
fn instante(fecha: NaiveDate, minutos: u32) -> DateTime<FixedOffset> {
    let medianoche = fecha.and_hms_opt(0, 0, 0).unwrap_or_default();
    let local = medianoche + TimeDelta::minutes(i64::from(minutos));
    bogota()
        .from_local_datetime(&local)
        .single()
        .unwrap_or_else(|| bogota().from_utc_datetime(&local))
}

fn dias(fecha: NaiveDate, n: i64) -> NaiveDate {
    if n >= 0 {
        fecha
            .checked_add_days(Days::new(n.unsigned_abs()))
            .unwrap_or(fecha)
    } else {
        fecha
            .checked_sub_days(Days::new(n.unsigned_abs()))
            .unwrap_or(fecha)
    }
}

// ---------------------------------------------------------------------------------------------
// La API, en este mismo proceso.
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Api {
    app: Router,
    origen: String,
    sesion: Option<String>,
    contrasena: String,
    /// Cada petición sale de una IP distinta (`X-Forwarded-For`): los límites por IP son para
    /// personas, no para este comando.
    ip: u32,
}

impl Api {
    fn siguiente_ip(&mut self) -> String {
        self.ip = self.ip.wrapping_add(1);
        format!(
            "10.{}.{}.{}",
            (self.ip >> 16) & 255,
            (self.ip >> 8) & 255,
            (self.ip & 255).max(1)
        )
    }

    async fn enviar(
        &mut self,
        metodo: &Method,
        ruta: &str,
        cuerpo: Option<&Value>,
    ) -> Resultado<(StatusCode, header::HeaderMap, Value)> {
        let mut peticion = Request::builder()
            .method(metodo.clone())
            .uri(ruta)
            .header(header::ORIGIN, &self.origen)
            .header("x-forwarded-for", self.siguiente_ip());
        if let Some(token) = &self.sesion {
            peticion = peticion.header(header::COOKIE, format!("__Host-sesion={token}"));
        }
        let peticion = match cuerpo {
            Some(json) => peticion
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string()))?,
            None => peticion.body(Body::empty())?,
        };
        let respuesta = self.app.clone().oneshot(peticion).await?;
        let (partes, cuerpo) = respuesta.into_parts();
        let bytes = axum::body::to_bytes(cuerpo, usize::MAX).await?;
        Ok((
            partes.status,
            partes.headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    /// Inicia sesión con la administradora de los datos de prueba.
    async fn entrar(&mut self) -> Resultado<()> {
        self.sesion = None;
        let cuerpo = json!({ "correo": CORREO_ADMIN, "contrasena": self.contrasena, "recordarme": true });
        let (estado, cabeceras, json) = self
            .enviar(&Method::POST, "/api/v1/auth/sesion", Some(&cuerpo))
            .await?;
        if estado != StatusCode::OK {
            return Err(error(format!("no se pudo iniciar sesión: {estado} {json}")));
        }
        let cookie = cabeceras
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("__Host-sesion="))
            .and_then(|v| v.split(';').next())
            .ok_or_else(|| error("el inicio de sesión no devolvió la cookie"))?;
        self.sesion = Some(cookie.to_owned());
        Ok(())
    }

    /// Una petición; si la sesión venció (el reloj saltó días), entra de nuevo y la repite.
    async fn pedir(
        &mut self,
        metodo: Method,
        ruta: &str,
        cuerpo: Option<Value>,
    ) -> Resultado<(StatusCode, Value)> {
        let (estado, _, json) = self.enviar(&metodo, ruta, cuerpo.as_ref()).await?;
        if estado == StatusCode::UNAUTHORIZED && !ruta.starts_with("/api/v1/auth") {
            self.entrar().await?;
            let (estado, _, json) = self.enviar(&metodo, ruta, cuerpo.as_ref()).await?;
            return Ok((estado, json));
        }
        Ok((estado, json))
    }

    async fn exigir(
        &mut self,
        metodo: Method,
        ruta: &str,
        cuerpo: Option<Value>,
        esperado: u16,
    ) -> Resultado<Value> {
        let (estado, json) = self.pedir(metodo.clone(), ruta, cuerpo).await?;
        if estado.as_u16() != esperado {
            return Err(error(format!("{metodo} {ruta} → {estado} {json}")));
        }
        Ok(json)
    }

    async fn get(&mut self, ruta: &str) -> Resultado<Value> {
        self.exigir(Method::GET, ruta, None, 200).await
    }
}

fn id(json: &Value) -> Resultado<i64> {
    json.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| error(format!("la respuesta no trae id: {json}")))
}

// ---------------------------------------------------------------------------------------------
// El comando.
// ---------------------------------------------------------------------------------------------

/// Resumen de lo que se cargó (y de lo que no se pudo).
#[derive(Debug, Default)]
struct Cuenta {
    citas: u32,
    en_linea: u32,
    ventas: u32,
    inasistencias: u32,
    canceladas: u32,
    cierres: u32,
    omitidas: u32,
}

struct Carga {
    api: Api,
    reloj: Arc<RelojDePruebas>,
    composicion: Composicion,
    azar: Azar,
    hoy: NaiveDate,
    inicio: NaiveDate,
    servicios: Vec<(i64, i64)>,
    barberos: Vec<i64>,
    /// Días fuera de servicio de cada barbero (antes de entrar, ausencias).
    ausente: Vec<HashSet<NaiveDate>>,
    desde: Vec<NaiveDate>,
    clientes: Vec<Cliente>,
    ocupado: HashMap<(usize, NaiveDate), [bool; 48]>,
    festivos: HashMap<NaiveDate, String>,
    metodos: HashMap<String, i64>,
    politica: i64,
    /// Desde cuándo rige la política: antes, no hay reservas en línea (se agenda por el panel).
    politica_desde: DateTime<Utc>,
    cuenta: Cuenta,
}

impl std::fmt::Debug for Carga {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Carga")
    }
}

pub(crate) async fn cargar(configuracion: &Configuracion) -> Resultado<()> {
    // 1. Seguros.
    let celulares: Vec<String> = (0..CLIENTES).map(|i| (PRIMER_CELULAR + i).to_string()).collect();
    if configuracion.whatsapp.cloud_api.is_some() {
        if configuracion.whatsapp.solo_a.is_empty() {
            return Err(error(
                "Los WhatsApp salen por Meta y los clientes de prueba tienen celulares inventados. Defina DIAMANTE_WHATSAPP_SOLO_A con los celulares del equipo (los únicos que recibirán WhatsApp de verdad), despliegue y vuelva a correr el comando.",
            ));
        }
        if let Some(c) = configuracion
            .whatsapp
            .solo_a
            .iter()
            .find(|c| celulares.contains(c))
        {
            return Err(error(format!(
                "DIAMANTE_WHATSAPP_SOLO_A incluye {c}, un celular de los datos de prueba"
            )));
        }
    }

    // 2. La aplicación en este proceso, con su propio reloj y sin límites de uso.
    let mut ajustada = configuracion.clone();
    ajustada.limites_de_uso = false;
    ajustada.confiar_proxy = true;
    ajustada.rutas_de_pruebas = false;
    ajustada.worker_en_api = false;
    let pool = conectar(&ajustada.base_datos.url, ajustada.base_datos.max_conexiones).await?;
    let reloj = Arc::new(RelojDePruebas::default());
    let composicion = Composicion::con_reloj_de_pruebas(pool, reloj.clone(), &ajustada)?;
    let app = enrutador::construir(&composicion);
    let origen = ajustada
        .origenes_permitidos
        .first()
        .cloned()
        .ok_or_else(|| error("sin origen permitido"))?;

    let hoy = Utc::now().with_timezone(&bogota()).date_naive();
    let inicio = dias(hoy, -i64::try_from(DIAS_DE_HISTORIA)?);
    reloj.fijar(instante(dias(inicio, -2), 8 * 60).with_timezone(&Utc));

    // 3. La administradora de los datos de prueba.
    let contrasena = admin_de_prueba(&composicion).await?;
    let mut api = Api {
        app,
        origen,
        sesion: None,
        contrasena,
        ip: 0,
    };
    api.entrar().await?;

    let ya = api
        .get(&format!("/api/v1/clientes/buscar?q={}", celulares[0]))
        .await?;
    if ya.as_array().is_some_and(|l| !l.is_empty()) {
        desactivar_admin(&composicion).await?;
        return Err(error(
            "Esta base ya tiene los datos de prueba (está el cliente 3209990001): no se cargan dos veces.",
        ));
    }

    let mut carga = Carga {
        api,
        reloj,
        composicion,
        azar: Azar(0x00D1_A3A7_E0B4_4B3E),
        hoy,
        inicio,
        servicios: Vec::new(),
        barberos: Vec::new(),
        ausente: Vec::new(),
        desde: Vec::new(),
        clientes: Vec::new(),
        ocupado: HashMap::new(),
        festivos: HashMap::new(),
        metodos: HashMap::new(),
        politica: 0,
        politica_desde: Utc::now(),
        cuenta: Cuenta::default(),
    };
    let resultado = carga.ejecutar(&celulares).await;
    carga.reloj.restablecer();
    let _ = carga.api.pedir(Method::DELETE, "/api/v1/auth/sesion", None).await;
    desactivar_admin(&carga.composicion).await?;
    carga.composicion.pool.close().await;
    resultado
}

/// Crea (o reactiva) la administradora de los datos de prueba con una contraseña nueva.
async fn admin_de_prueba(composicion: &Composicion) -> Resultado<String> {
    sqlx::query("UPDATE identidad.usuario SET activo = true WHERE correo = $1")
        .bind(CORREO_ADMIN)
        .execute(&composicion.pool)
        .await?;
    let enlace = match composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, CORREO_ADMIN, "Datos de prueba")
        .await
    {
        Ok(invitacion) => invitacion.enlace,
        Err(_) => composicion.identidad.enlace_de_acceso(CORREO_ADMIN).await?,
    };
    let token = enlace
        .split("#token=")
        .nth(1)
        .ok_or_else(|| error(format!("enlace sin token: {enlace}")))?
        .to_owned();
    let (contrasena, _) = plataforma::seguridad::generar_token()?;
    let ruta = if enlace.contains("/acceso/activar") {
        "/api/v1/auth/activacion"
    } else {
        "/api/v1/auth/recuperacion/confirmar"
    };
    // Se activa por la API (sin sesión todavía): un `Api` temporal sin cookie.
    let mut temporal = Api {
        app: enrutador::construir(composicion),
        origen: composicion.origenes.0.first().cloned().unwrap_or_default(),
        sesion: None,
        contrasena: String::new(),
        ip: 9_000,
    };
    temporal
        .exigir(
            Method::POST,
            ruta,
            Some(json!({ "token": token, "contrasena": contrasena })),
            204,
        )
        .await?;
    Ok(contrasena)
}

async fn desactivar_admin(composicion: &Composicion) -> Resultado<()> {
    sqlx::query("UPDATE identidad.usuario SET activo = false WHERE correo = $1")
        .bind(CORREO_ADMIN)
        .execute(&composicion.pool)
        .await?;
    Ok(())
}

impl Carga {
    fn a_las(&self, fecha: NaiveDate, minutos: u32) {
        self.reloj.fijar(instante(fecha, minutos).with_timezone(&Utc));
    }

    fn ahora_real() -> DateTime<FixedOffset> {
        Utc::now().with_timezone(&bogota())
    }

    /// Lo que la API dejó en la cola de WhatsApp se envía ya, a la hora simulada (con la lista
    /// de `DIAMANTE_WHATSAPP_SOLO_A`, a los celulares de prueba se les simula).
    async fn enviar_whatsapp(&self) {
        if let Err(e) = self.composicion.notificaciones.enviar_whatsapp_pendientes().await {
            tracing::warn!(error = %e, "no se pudo vaciar la cola de WhatsApp");
        }
    }

    async fn ejecutar(&mut self, celulares: &[String]) -> Resultado<()> {
        println!(
            "Datos de prueba: {} → {} (hoy {})",
            self.inicio,
            dias(self.hoy, 21),
            self.hoy
        );
        self.base().await?;
        self.equipo().await?;
        self.planear_clientes(celulares);
        // El bloqueo de Mateo (en 3 días, 4 a 5 p. m.) se aparta antes de repartir las citas.
        let fila = self.ocupado.entry((1, dias(self.hoy, 3))).or_insert([false; 48]);
        fila[32] = true;
        fila[33] = true;
        let visitas = self.planear_visitas();
        println!(
            "  {} citas planeadas para {} clientes",
            visitas.len(),
            self.clientes.len()
        );
        self.historia(&visitas).await?;
        self.por_venir(&visitas).await?;
        self.extras().await?;
        let c = &self.cuenta;
        println!(
            "✔ Listo: {} citas ({} en línea), {} ventas, {} inasistencias, {} canceladas, {} cierres de caja; {} pasos omitidos.",
            c.citas, c.en_linea, c.ventas, c.inasistencias, c.canceladas, c.cierres, c.omitidas
        );
        Ok(())
    }

    /// Política, festivos, métodos de pago y servicios.
    async fn base(&mut self) -> Resultado<()> {
        // La política vigente; en una base sin política se publica la 1.0, vigente desde antes de
        // la historia (en una que ya la tiene no se toca: antes de su fecha se agenda por el panel).
        let politicas = self.api.get("/api/v1/negocio/politicas").await?;
        let vigente = politicas.as_array().and_then(|l| {
            l.iter()
                .find(|p| p["tipo"] == "tratamiento_datos" && p["vigente"] == true)
                .cloned()
        });
        let politica = match vigente {
            Some(p) => p,
            None => {
                self.api
                    .exigir(
                        Method::POST,
                        "/api/v1/negocio/politicas",
                        Some(json!({
                            "tipo": "tratamiento_datos",
                            "version": "1.0",
                            "url": "/legal/tratamiento-de-datos",
                            "vigente_desde": instante(dias(self.inicio, -3), 8 * 60).with_timezone(&Utc),
                        })),
                        201,
                    )
                    .await?
            }
        };
        self.politica = id(&politica)?;
        self.politica_desde = politica["vigente_desde"]
            .as_str()
            .and_then(|d| d.parse::<DateTime<Utc>>().ok())
            .ok_or_else(|| error(format!("política sin vigente_desde: {politica}")))?;
        for anio in [self.inicio.year(), self.hoy.year(), dias(self.hoy, 30).year()]
            .into_iter()
            .collect::<HashSet<_>>()
        {
            self.api
                .exigir(
                    Method::POST,
                    &format!("/api/v1/negocio/dias-especiales/festivos/{anio}"),
                    Some(json!({})),
                    200,
                )
                .await?;
        }
        let especiales = self
            .api
            .get(&format!(
                "/api/v1/negocio/dias-especiales?desde={}&hasta={}",
                self.inicio,
                dias(self.hoy, 30)
            ))
            .await?;
        for dia in especiales.as_array().into_iter().flatten() {
            if let (Some(fecha), Some(tipo)) = (dia["fecha"].as_str(), dia["tipo"].as_str())
                && let Ok(fecha) = fecha.parse::<NaiveDate>()
            {
                self.festivos.insert(fecha, tipo.to_owned());
            }
        }
        let catalogos = self.api.get("/api/v1/ventas/catalogos").await?;
        for metodo in catalogos["metodos_pago"].as_array().into_iter().flatten() {
            if let (Some(codigo), Some(id)) = (metodo["codigo"].as_str(), metodo["id"].as_i64()) {
                self.metodos.insert(codigo.to_owned(), id);
            }
        }

        let mut ids: Vec<i64> = Vec::new();
        for (orden, s) in SERVICIOS.iter().enumerate() {
            let mut cuerpo = json!({
                "categoria_id": s.categoria,
                "nombre": s.nombre,
                "descripcion": s.descripcion,
                "precio": s.precio,
                "agendable_en_linea": s.peso > 0,
                "visible_en_landing": true,
                "destacado": s.etiqueta.is_some(),
                "etiqueta": s.etiqueta,
                "orden": orden + 1,
            });
            cuerpo["duracion_min"] = json!(s.duracion);
            if !s.componentes.is_empty() {
                cuerpo["tipo"] = json!("combo");
                cuerpo["componentes"] = json!(
                    s.componentes
                        .iter()
                        .filter_map(|i| ids.get(*i))
                        .collect::<Vec<_>>()
                );
            }
            let creado = self
                .api
                .exigir(Method::POST, "/api/v1/servicios", Some(cuerpo), 201)
                .await?;
            ids.push(id(&creado)?);
            self.servicios.push((id(&creado)?, s.precio));
        }
        println!(
            "  {} servicios y combos, festivos y política de datos",
            SERVICIOS.len()
        );
        Ok(())
    }

    /// Los cinco barberos, con su horario; una incapacidad pasada.
    async fn equipo(&mut self) -> Resultado<()> {
        for (i, b) in BARBEROS.iter().enumerate() {
            let ingreso = dias(self.hoy, -i64::try_from(b.dias_en_el_equipo)?);
            // Se registra el día que entra (o al empezar la historia).
            self.a_las(dias(ingreso.max(self.inicio), -1), 9 * 60);
            let servicios: Vec<i64> = b
                .servicios
                .iter()
                .filter_map(|s| self.servicios.get(*s).map(|x| x.0))
                .collect();
            let horario: Vec<Value> = b
                .franjas
                .iter()
                .map(|(dia, desde, hasta)| json!({ "dia_semana": dia, "inicio": desde, "fin": hasta }))
                .collect();
            let indice = u64::try_from(i)?;
            let correo = format!(
                "{}.{}@example.com",
                sin_tildes(b.nombres),
                sin_tildes(b.apellidos)
            );
            let creado = self
                .api
                .exigir(
                    Method::POST,
                    "/api/v1/barberos",
                    Some(json!({
                        "tipo_documento": "CC",
                        "numero_documento": format!("{}", 1_020_304_050 + indice * 1_111),
                        "nombres": b.nombres,
                        "apellidos": b.apellidos,
                        "fecha_nacimiento": format!("{}-0{}-1{}", 1988 + indice * 2, indice + 2, indice + 1),
                        "celular": format!("{}", 3_209_998_001 + indice),
                        "correo": correo,
                        "especialidad": b.especialidad,
                        "anios_experiencia": b.anios,
                        "biografia": b.biografia,
                        "fecha_ingreso": ingreso,
                        "tipo_vinculacion": "prestacion_servicios",
                        "porcentaje_comision": b.comision,
                        "periodo_liquidacion": "quincenal",
                        "servicios": servicios,
                        "horario": horario,
                        "visibilidad": { "mostrar_en_landing": true, "recibe_citas_en_linea": true, "acceso_panel": b.acceso_panel },
                    })),
                    201,
                )
                .await?;
            let barbero = id(&creado)?;
            self.barberos.push(barbero);
            self.desde.push(ingreso);
            let mut fuera: HashSet<NaiveDate> = HashSet::new();
            let mut dia = self.inicio;
            while dia < ingreso {
                fuera.insert(dia);
                dia = dias(dia, 1);
            }
            self.ausente.push(fuera);
            if b.acceso_panel {
                let clave = self.cuenta_de_barbero(&correo).await?;
                println!("  Barbero con acceso al panel: {correo} / {clave}");
            }
        }
        // Felipe estuvo incapacitado hace unas semanas.
        let (desde, hasta) = (dias(self.hoy, -40), dias(self.hoy, -37));
        self.a_las(dias(desde, -1), 18 * 60);
        let felipe = self
            .barberos
            .get(2)
            .copied()
            .ok_or_else(|| error("falta Felipe"))?;
        self.api
            .exigir(
                Method::POST,
                &format!("/api/v1/barberos/{felipe}/ausencias"),
                Some(json!({ "tipo": "incapacidad", "desde": desde, "hasta": hasta, "observacion": "Incapacidad médica por 4 días." })),
                201,
            )
            .await?;
        let mut dia = desde;
        while dia <= hasta {
            if let Some(f) = self.ausente.get_mut(2) {
                f.insert(dia);
            }
            dia = dias(dia, 1);
        }
        println!("  {} barberos con su horario", BARBEROS.len());
        Ok(())
    }

    /// Activa la cuenta de un barbero con una contraseña aleatoria y la devuelve.
    async fn cuenta_de_barbero(&mut self, correo: &str) -> Resultado<String> {
        let enlace = self.composicion.identidad.enlace_de_acceso(correo).await?;
        let token = enlace
            .split("#token=")
            .nth(1)
            .ok_or_else(|| error("enlace sin token"))?
            .to_owned();
        let (clave, _) = plataforma::seguridad::generar_token()?;
        let clave: String = clave.chars().take(20).collect();
        self.api
            .exigir(
                Method::POST,
                "/api/v1/auth/activacion",
                Some(json!({ "token": token, "contrasena": clave })),
                204,
            )
            .await?;
        Ok(clave)
    }

    fn planear_clientes(&mut self, celulares: &[String]) {
        let pesos: Vec<u32> = SERVICIOS.iter().map(|s| s.peso).collect();
        for (i, celular) in celulares.iter().enumerate() {
            let nombre = format!(
                "{} {} {}",
                NOMBRES[self.azar.indice(NOMBRES.len())],
                APELLIDOS[self.azar.indice(APELLIDOS.len())],
                APELLIDOS[self.azar.indice(APELLIDOS.len())]
            );
            let correo = self.azar.prob(0.4).then(|| {
                format!(
                    "{}{}@example.com",
                    sin_tildes(nombre.split(' ').next().unwrap_or("cliente")),
                    i + 1
                )
            });
            let nacimiento = if i < 4 {
                // Cumpleaños esta semana (para las alertas del panel).
                let dia = dias(self.hoy, i64::try_from(i).unwrap_or(0) + 1);
                NaiveDate::from_ymd_opt(1990 + i32::try_from(i).unwrap_or(0), dia.month(), dia.day())
            } else if self.azar.prob(0.5) {
                NaiveDate::from_ymd_opt(
                    1970 + i32::try_from(self.azar.entre(0, 38)).unwrap_or(0),
                    u32::try_from(self.azar.entre(1, 12)).unwrap_or(1),
                    u32::try_from(self.azar.entre(1, 28)).unwrap_or(1),
                )
            } else {
                None
            };
            let preferencias = self
                .azar
                .prob(0.25)
                .then(|| PREFERENCIAS[self.azar.indice(PREFERENCIAS.len())]);
            let servicio = self.azar.ponderado(&pesos);
            let barbero = self.azar.ponderado(&[30, 25, 20, 15, 10]);
            self.clientes.push(Cliente {
                id: None,
                nombre,
                celular: celular.clone(),
                correo,
                nacimiento,
                preferencias,
                servicio,
                barbero,
                alta_en_linea: self.azar.prob(0.35),
                en_linea: 2,
            });
        }
    }

    fn franjas(&self, barbero: usize, fecha: NaiveDate) -> Vec<(u32, u32)> {
        if self.ausente.get(barbero).is_some_and(|f| f.contains(&fecha)) {
            return Vec::new();
        }
        let dia = match self.festivos.get(&fecha).map(String::as_str) {
            Some("cerrado") => return Vec::new(),
            Some("festivo") => 7,
            _ => u8::try_from(fecha.weekday().number_from_monday()).unwrap_or(1),
        };
        BARBEROS
            .get(barbero)
            .map(|b| {
                b.franjas
                    .iter()
                    .filter(|(d, _, _)| *d == dia)
                    .map(|(_, desde, hasta)| (minutos(desde), minutos(hasta)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ofrece(barbero: usize, servicio: usize) -> bool {
        BARBEROS
            .get(barbero)
            .is_some_and(|b| b.servicios.contains(&servicio))
    }

    /// Un hueco libre para `duracion` minutos (en bloques de 30), o `None`.
    fn hueco(&mut self, barbero: usize, fecha: NaiveDate, duracion: u32) -> Option<u32> {
        let bloques = duracion.div_ceil(30);
        let ocupado = self
            .ocupado
            .get(&(barbero, fecha))
            .copied()
            .unwrap_or([false; 48]);
        let mut candidatos = Vec::new();
        for (desde, hasta) in self.franjas(barbero, fecha) {
            let mut t = desde.div_ceil(30) * 30;
            while t + bloques * 30 <= hasta {
                let i = usize::try_from(t / 30).unwrap_or(0);
                let n = usize::try_from(bloques).unwrap_or(1);
                if ocupado.get(i..i + n).is_some_and(|b| b.iter().all(|x| !x)) {
                    candidatos.push(t);
                }
                t += 30;
            }
        }
        if candidatos.is_empty() {
            return None;
        }
        let t = candidatos[self.azar.indice(candidatos.len())];
        let i = usize::try_from(t / 30).unwrap_or(0);
        let n = usize::try_from(bloques).unwrap_or(1);
        let fila = self.ocupado.entry((barbero, fecha)).or_insert([false; 48]);
        for b in fila.iter_mut().skip(i).take(n) {
            *b = true;
        }
        Some(t)
    }

    /// Las fechas de visita de cada cliente según su perfil y un hueco para cada una.
    fn planear_visitas(&mut self) -> Vec<Visita> {
        let historia = i64::try_from(DIAS_DE_HISTORIA).unwrap_or(84);
        let futuro = i64::try_from(DIAS_POR_VENIR).unwrap_or(21);
        let mut deseadas: Vec<(NaiveDate, usize)> = Vec::new();
        for c in 0..self.clientes.len() {
            let perfil = match c % 20 {
                0..=4 => Perfil::Frecuente,
                5..=10 => Perfil::Recurrente,
                11..=15 => Perfil::Nuevo,
                16..=18 => Perfil::Inactivo,
                _ => Perfil::SinVisitas,
            };
            let mut fechas: Vec<i64> = Vec::new();
            match perfil {
                Perfil::Frecuente => {
                    let mut d = -historia + i64::try_from(self.azar.entre(0, 12)).unwrap_or(0);
                    while d <= futuro {
                        fechas.push(d);
                        d += i64::try_from(self.azar.entre(8, 15)).unwrap_or(10);
                    }
                }
                Perfil::Recurrente => {
                    let n = self.azar.entre(3, 5);
                    let mut d = -historia + i64::try_from(self.azar.entre(0, 20)).unwrap_or(0);
                    for _ in 0..n {
                        fechas.push(d);
                        d += i64::try_from(self.azar.entre(14, 28)).unwrap_or(20);
                    }
                }
                Perfil::Nuevo => {
                    let d = -i64::try_from(self.azar.entre(5, 40)).unwrap_or(10);
                    fechas.push(d);
                    if self.azar.prob(0.5) {
                        fechas.push(d + i64::try_from(self.azar.entre(12, 30)).unwrap_or(15));
                    }
                }
                Perfil::Inactivo => {
                    let mut d = -historia + i64::try_from(self.azar.entre(0, 6)).unwrap_or(0);
                    for _ in 0..self.azar.entre(2, 3) {
                        if d < -62 {
                            fechas.push(d);
                        }
                        d += i64::try_from(self.azar.entre(6, 10)).unwrap_or(8);
                    }
                }
                Perfil::SinVisitas => {
                    if self.azar.prob(0.5) {
                        fechas.push(i64::try_from(self.azar.entre(2, 15)).unwrap_or(5));
                    }
                }
            }
            deseadas.extend(
                fechas
                    .into_iter()
                    .filter(|d| *d <= futuro)
                    .map(|d| (dias(self.hoy, d), c)),
            );
        }
        deseadas.sort_by_key(|(f, c)| (*f, *c));

        let pesos: Vec<u32> = SERVICIOS.iter().map(|s| s.peso).collect();
        let mut visitas = Vec::new();
        for (fecha, c) in deseadas {
            let Some(cliente) = self.clientes.get(c) else {
                continue;
            };
            let servicio = if self.azar.prob(0.8) {
                cliente.servicio
            } else {
                self.azar.ponderado(&pesos)
            };
            let preferido = cliente.barbero;
            let duracion = SERVICIOS.get(servicio).map_or(30, |s| s.duracion);
            // Su barbero; si no puede ese día, otro; si nadie, el día siguiente (hasta 3).
            let mut colocada = None;
            'dias: for corrimiento in 0..4 {
                let dia = dias(fecha, corrimiento);
                if dia < self.inicio || dia > dias(self.hoy, i64::try_from(DIAS_POR_VENIR).unwrap_or(21)) {
                    break;
                }
                let mut orden: Vec<usize> = (0..BARBEROS.len()).collect();
                orden.sort_by_key(|b| (*b != preferido, self.azar.siguiente() % 7));
                for b in orden {
                    if Self::ofrece(b, servicio)
                        && let Some(inicio) = self.hueco(b, dia, duracion)
                    {
                        colocada = Some(Visita {
                            cliente: c,
                            barbero: b,
                            servicio,
                            fecha: dia,
                            inicio,
                        });
                        break 'dias;
                    }
                }
            }
            if let Some(v) = colocada {
                visitas.push(v);
            }
        }
        visitas.sort_by_key(|v| (v.fecha, v.inicio));
        visitas
    }

    /// Registra al cliente por el panel si aún no existe (a la hora simulada actual).
    async fn asegurar_cliente(&mut self, c: usize) -> Resultado<Option<i64>> {
        let Some(cliente) = self.clientes.get(c) else {
            return Ok(None);
        };
        if let Some(id) = cliente.id {
            return Ok(Some(id));
        }
        let cuerpo = json!({
            "nombre": cliente.nombre,
            "celular": cliente.celular,
            "correo": cliente.correo,
            "fecha_nacimiento": cliente.nacimiento,
            "preferencias": cliente.preferencias,
        });
        let creado = self
            .api
            .exigir(Method::POST, "/api/v1/clientes", Some(cuerpo), 201)
            .await?;
        let nuevo = id(&creado)?;
        if let Some(cliente) = self.clientes.get_mut(c) {
            cliente.id = Some(nuevo);
        }
        Ok(Some(nuevo))
    }

    /// Crea la cita (en línea o por el panel) a la hora simulada actual. Devuelve su id y si
    /// quedó «Sin confirmar».
    async fn crear_cita(&mut self, v: &Visita, en_linea: bool) -> Resultado<Option<(i64, bool)>> {
        let barbero = self
            .barberos
            .get(v.barbero)
            .copied()
            .ok_or_else(|| error("barbero"))?;
        let servicio = self
            .servicios
            .get(v.servicio)
            .map(|s| s.0)
            .ok_or_else(|| error("servicio"))?;
        let inicio = instante(v.fecha, v.inicio).to_rfc3339();
        if en_linea {
            let Some(cliente) = self.clientes.get_mut(v.cliente) else {
                return Ok(None);
            };
            // El intento cuenta aunque falle: el límite de códigos por celular es por hora real.
            cliente.en_linea = cliente.en_linea.saturating_sub(1);
            let celular = cliente.celular.clone();
            let (nombre, correo, nacimiento) =
                (cliente.nombre.clone(), cliente.correo.clone(), cliente.nacimiento);
            let (estado, codigo) = self
                .api
                .pedir(
                    Method::POST,
                    "/api/v1/publico/verificaciones",
                    Some(json!({ "celular": celular })),
                )
                .await?;
            if estado != StatusCode::ACCEPTED {
                tracing::warn!(%estado, %codigo, "sin código de verificación: se agenda por el panel");
                return Box::pin(self.crear_cita(v, false)).await;
            }
            let verificacion = id(&codigo)?;
            let enviado: Option<String> = sqlx::query_scalar(
                "SELECT variables->>'codigo' FROM notificaciones.mensaje
                 WHERE destinatario = $1 AND plantilla = 'codigo_verificacion' ORDER BY id DESC LIMIT 1",
            )
            .bind(&celular)
            .fetch_optional(&self.composicion.pool)
            .await?
            .flatten();
            let codigo =
                enviado.ok_or_else(|| error("no se encontró el código de verificación en la cola"))?;
            self.enviar_whatsapp().await;
            let confirmado = self
                .api
                .exigir(
                    Method::POST,
                    &format!("/api/v1/publico/verificaciones/{verificacion}/confirmar"),
                    Some(json!({ "codigo": codigo })),
                    200,
                )
                .await?;
            let (estado, creada) = self
                .api
                .pedir(
                    Method::POST,
                    "/api/v1/publico/citas",
                    Some(json!({
                        "comprobante": confirmado["comprobante"],
                        "servicio_id": servicio,
                        "barbero_id": barbero,
                        "inicio": inicio,
                        "cliente": { "nombre": nombre, "correo": correo, "fecha_nacimiento": nacimiento },
                        "autoriza_tratamiento_datos": true,
                        "politica_version_id": self.politica,
                    })),
                )
                .await?;
            if estado != StatusCode::CREATED {
                tracing::warn!(%estado, %creada, "reserva en línea rechazada: se agenda por el panel");
                return Box::pin(self.crear_cita(v, false)).await;
            }
            self.enviar_whatsapp().await;
            if self.clientes.get(v.cliente).is_some_and(|c| c.id.is_none()) {
                let encontrados = self
                    .api
                    .get(&format!("/api/v1/clientes/buscar?q={celular}"))
                    .await?;
                let id = encontrados
                    .as_array()
                    .and_then(|l| l.first())
                    .and_then(|c| c["id"].as_i64());
                if let Some(cliente) = self.clientes.get_mut(v.cliente) {
                    cliente.id = id;
                }
            }
            self.cuenta.citas += 1;
            self.cuenta.en_linea += 1;
            let cita = creada["cita"]["id"]
                .as_i64()
                .ok_or_else(|| error("reserva sin id"))?;
            return Ok(Some((cita, creada["cita"]["estado"] == "sin_confirmar")));
        }
        let Some(cliente) = self.asegurar_cliente(v.cliente).await? else {
            return Ok(None);
        };
        let nota = self
            .azar
            .prob(0.1)
            .then_some("Llega 5 minutos tarde, avisó por WhatsApp.");
        let (estado, creada) = self
            .api
            .pedir(
                Method::POST,
                "/api/v1/agenda/citas",
                Some(json!({ "cliente_id": cliente, "servicio_id": servicio, "barbero_id": barbero, "inicio": inicio, "nota": nota })),
            )
            .await?;
        if estado != StatusCode::CREATED {
            tracing::warn!(%estado, %creada, "cita omitida");
            self.cuenta.omitidas += 1;
            return Ok(None);
        }
        self.enviar_whatsapp().await;
        self.cuenta.citas += 1;
        Ok(Some((id(&creada)?, creada["estado"] == "sin_confirmar")))
    }

    /// ¿Se reserva en línea? Solo si le quedan reservas y la fecha está dentro de la ventana.
    fn en_linea(&mut self, v: &Visita) -> bool {
        if self.reloj.ahora() < self.politica_desde + TimeDelta::minutes(1) {
            return false;
        }
        let quedan = self
            .clientes
            .get(v.cliente)
            .is_some_and(|c| c.en_linea > 0 && (c.id.is_some() || c.alta_en_linea));
        let primera = self
            .clientes
            .get(v.cliente)
            .is_some_and(|c| c.id.is_none() && c.alta_en_linea);
        quedan && (primera || self.azar.prob(PROBABILIDAD_EN_LINEA))
    }

    /// Cobra la cita (o una venta sin cita si `cita` es `None`) a la hora simulada actual.
    async fn cobrar(
        &mut self,
        cita: Option<i64>,
        v: &Visita,
        cliente: Option<i64>,
    ) -> Resultado<Option<i64>> {
        let mut lineas = vec![v.servicio];
        if self.azar.prob(0.15) {
            lineas.push(EXTRAS[self.azar.indice(EXTRAS.len())]);
        }
        lineas.retain(|s| Self::ofrece(v.barbero, *s));
        let subtotal: i64 = lineas
            .iter()
            .filter_map(|s| self.servicios.get(*s))
            .map(|s| s.1)
            .sum();
        let ids: Vec<i64> = lineas
            .iter()
            .filter_map(|s| self.servicios.get(*s))
            .map(|s| s.0)
            .collect();
        let (descuento, valor) = match self.azar.entre(0, 99) {
            0..=4 => (
                Some(json!({ "descuento_id": 1, "motivo_descuento_id": 1 })),
                subtotal / 10,
            ),
            5..=6 => (
                Some(json!({ "descuento_id": 2, "motivo_descuento_id": 2 })),
                subtotal * 15 / 100,
            ),
            7 if subtotal > 10_000 => (
                Some(json!({ "descuento_id": 3, "motivo_descuento_id": 3 })),
                5_000,
            ),
            _ => (None, 0),
        };
        let propina = match self.azar.entre(0, 9) {
            0..=3 => 0,
            4..=6 => 2_000,
            7..=8 => 5_000,
            _ => 10_000,
        };
        let total = subtotal - valor + propina;
        let metodo = |codigo: &str, s: &Self| s.metodos.get(codigo).copied().unwrap_or(1);
        let codigos = ["efectivo", "nequi", "tarjeta", "daviplata", "transferencia"];
        let principal = codigos[self.azar.ponderado(&[45, 30, 15, 5, 5])];
        let pagos = if self.azar.prob(0.1) && total > 20_000 {
            let efectivo = (total / 2 / 1_000) * 1_000;
            json!([
                { "metodo_pago_id": metodo("efectivo", self), "monto": efectivo },
                { "metodo_pago_id": metodo("nequi", self), "monto": total - efectivo },
            ])
        } else {
            json!([{ "metodo_pago_id": metodo(principal, self), "monto": total }])
        };
        let barbero = self.barberos.get(v.barbero).copied();
        let mut cuerpo =
            json!({ "servicio_ids": ids, "propina": propina, "pagos": pagos, "barbero_id": barbero });
        if let Some(d) = descuento {
            cuerpo["descuento"] = d;
        }
        match cita {
            Some(cita) => cuerpo["cita_id"] = json!(cita),
            None => {
                cuerpo["cliente"] = match cliente {
                    Some(id) => json!({ "tipo": "registrado", "id": id }),
                    None => json!({ "tipo": "ocasional" }),
                };
            }
        }
        let (estado, venta) = self
            .api
            .pedir(Method::POST, "/api/v1/ventas", Some(cuerpo))
            .await?;
        if estado != StatusCode::CREATED {
            tracing::warn!(%estado, %venta, "venta omitida");
            self.cuenta.omitidas += 1;
            return Ok(None);
        }
        self.cuenta.ventas += 1;
        Ok(Some(id(&venta)?))
    }

    /// Las 12 semanas pasadas y lo que ya pasó hoy.
    async fn historia(&mut self, visitas: &[Visita]) -> Resultado<()> {
        let ahora = Self::ahora_real();
        let ayer = dias(self.hoy, -1);
        let mut anuladas = 0;
        let mut pendientes_de_cierre = 0;
        let mut dia = self.inicio;
        while dia <= self.hoy {
            let del_dia: Vec<Visita> = visitas.iter().filter(|v| v.fecha == dia).cloned().collect();
            for v in &del_dia {
                let inicio = instante(v.fecha, v.inicio);
                let duracion = SERVICIOS.get(v.servicio).map_or(30, |s| s.duracion);
                let fin = inicio + TimeDelta::minutes(i64::from(duracion));
                // Se agenda entre 1 y 6 días antes, en horario de oficina.
                let antes = i64::try_from(self.azar.entre(1, 6))?;
                let creada_a =
                    (u32::try_from(self.azar.entre(8, 20))?) * 60 + u32::try_from(self.azar.entre(0, 59))?;
                let agendada = instante(dias(v.fecha, -antes), creada_a);
                self.a_las(dias(v.fecha, -antes), creada_a);
                let en_linea = self.en_linea(v);
                let Some((cita, sin_confirmar)) = self.crear_cita(v, en_linea).await? else {
                    continue;
                };
                if sin_confirmar && self.azar.prob(0.7) {
                    self.a_las(dias(v.fecha, -antes), creada_a + 60);
                    let _ = self
                        .api
                        .pedir(
                            Method::POST,
                            &format!("/api/v1/agenda/citas/{cita}/confirmar"),
                            None,
                        )
                        .await?;
                }
                if dia == self.hoy && inicio > ahora {
                    continue; // Aún no llega: queda por atender.
                }
                if dia >= dias(self.hoy, -3)
                    && dia < self.hoy
                    && pendientes_de_cierre < 2
                    && self.azar.prob(0.35)
                {
                    pendientes_de_cierre += 1;
                    continue; // Sin cerrar: aparece en «pendientes de cierre».
                }
                let suerte = self.azar.entre(0, 99);
                if suerte < 6 {
                    // Inasistencia.
                    self.reloj
                        .fijar((inicio + TimeDelta::minutes(25)).with_timezone(&Utc));
                    let (estado, _) = self
                        .api
                        .pedir(
                            Method::POST,
                            &format!("/api/v1/agenda/citas/{cita}/inasistencia"),
                            None,
                        )
                        .await?;
                    if estado == StatusCode::OK {
                        self.cuenta.inasistencias += 1;
                    }
                    continue;
                }
                if suerte < 13 {
                    // Cancelada unas horas antes.
                    let horas = i64::try_from(self.azar.entre(3, 20))?;
                    let cancelada = (inicio - TimeDelta::hours(horas)).max(agendada + TimeDelta::minutes(90));
                    self.reloj.fijar(cancelada.with_timezone(&Utc));
                    let motivo = if suerte < 11 {
                        "solicitud_cliente"
                    } else {
                        "barbero_no_disponible"
                    };
                    let (estado, _) = self
                        .api
                        .pedir(
                            Method::POST,
                            &format!("/api/v1/agenda/citas/{cita}/cancelar"),
                            Some(json!({ "motivo": motivo, "avisar_cliente": true })),
                        )
                        .await?;
                    self.enviar_whatsapp().await;
                    if estado == StatusCode::OK {
                        self.cuenta.canceladas += 1;
                    }
                    continue;
                }
                // Atendida: empieza, termina y se cobra.
                self.reloj
                    .fijar((inicio + TimeDelta::minutes(2)).with_timezone(&Utc));
                let _ = self
                    .api
                    .pedir(
                        Method::POST,
                        &format!("/api/v1/agenda/citas/{cita}/iniciar"),
                        None,
                    )
                    .await?;
                if dia == self.hoy && fin > ahora {
                    continue; // En curso ahora mismo.
                }
                self.reloj
                    .fijar((fin + TimeDelta::minutes(4)).with_timezone(&Utc));
                let venta = self.cobrar(Some(cita), v, None).await?;
                // Dos ventas se anulan (se cobró otro servicio) y se vuelven a cobrar.
                if let Some(venta) = venta
                    && anuladas < 2
                    && dia > dias(self.inicio, 30)
                    && self.azar.prob(0.02)
                {
                    self.reloj
                        .fijar((fin + TimeDelta::minutes(12)).with_timezone(&Utc));
                    let (estado, _) = self
                        .api
                        .pedir(
                            Method::POST,
                            &format!("/api/v1/ventas/{venta}/anulacion"),
                            Some(json!({ "motivo": "Se registró un servicio distinto al que se hizo." })),
                        )
                        .await?;
                    if estado == StatusCode::OK {
                        anuladas += 1;
                        self.reloj
                            .fijar((fin + TimeDelta::minutes(15)).with_timezone(&Utc));
                        self.cobrar(Some(cita), v, None).await?;
                    }
                }
            }
            if dia < self.hoy {
                self.sin_cita(dia).await?;
            }
            if dia < ayer {
                self.cierre(dia).await?;
            }
            if dia.day() == 1 || dia == self.hoy {
                println!(
                    "  … {dia}: {} citas, {} ventas",
                    self.cuenta.citas, self.cuenta.ventas
                );
            }
            dia = dias(dia, 1);
        }
        Ok(())
    }

    /// Ventas sin cita (clientes que llegan sin reservar) en los huecos del día.
    async fn sin_cita(&mut self, dia: NaiveDate) -> Resultado<()> {
        for b in 0..BARBEROS.len() {
            for _ in 0..self.azar.entre(0, 2) {
                let servicio = [0, 1, 3, 8][self.azar.indice(4)];
                if !Self::ofrece(b, servicio) {
                    continue;
                }
                let duracion = SERVICIOS.get(servicio).map_or(30, |s| s.duracion);
                let Some(inicio) = self.hueco(b, dia, duracion) else {
                    continue;
                };
                let registrado = if self.azar.prob(0.3) {
                    let c = self.azar.indice(self.clientes.len());
                    self.clientes.get(c).and_then(|c| c.id)
                } else {
                    None
                };
                self.a_las(dia, inicio + duracion + 3);
                let v = Visita {
                    cliente: 0,
                    barbero: b,
                    servicio,
                    fecha: dia,
                    inicio,
                };
                self.cobrar(None, &v, registrado).await?;
            }
        }
        Ok(())
    }

    /// Cierre de caja del día con lo contado (a veces falta algo de efectivo).
    async fn cierre(&mut self, dia: NaiveDate) -> Resultado<()> {
        self.a_las(dia, 20 * 60 + 30);
        let caja = self.api.get(&format!("/api/v1/ventas/caja?fecha={dia}")).await?;
        if caja["numero_ventas"].as_i64().unwrap_or(0) == 0 {
            return Ok(());
        }
        let faltante = if self.azar.prob(0.1) { 2_000 } else { 0 };
        let efectivo = self.metodos.get("efectivo").copied();
        let contado: Vec<Value> = caja["por_metodo"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                let total = m["total"].as_i64().unwrap_or(0);
                let id = m["metodo_pago_id"].as_i64();
                let monto = if id == efectivo {
                    (total - faltante).max(0)
                } else {
                    total
                };
                json!({ "metodo_pago_id": id, "monto": monto })
            })
            .collect();
        let observacion = (faltante > 0).then_some("Faltaron $2.000 en efectivo; se revisan las vueltas.");
        let (estado, _) = self
            .api
            .pedir(
                Method::POST,
                "/api/v1/ventas/cierres",
                Some(json!({ "fecha": dia, "contado": contado, "observacion": observacion })),
            )
            .await?;
        if estado == StatusCode::CREATED {
            self.cuenta.cierres += 1;
        }
        Ok(())
    }

    /// Las citas de las próximas 3 semanas, agendadas en los últimos días.
    async fn por_venir(&mut self, visitas: &[Visita]) -> Resultado<()> {
        let ahora = Self::ahora_real();
        // Un bloqueo de Mateo antes de agendar (cita médica en 3 días).
        let mateo = self
            .barberos
            .get(1)
            .copied()
            .ok_or_else(|| error("falta Mateo"))?;
        let dia_bloqueo = dias(self.hoy, 3);
        if !self.franjas(1, dia_bloqueo).is_empty() {
            self.reloj.fijar((ahora - TimeDelta::days(5)).with_timezone(&Utc));
            let _ = self
                .api
                .pedir(
                    Method::POST,
                    &format!("/api/v1/barberos/{mateo}/bloqueos"),
                    Some(json!({ "fecha": dia_bloqueo, "desde": "16:00", "hasta": "17:00", "motivo": "cita_medica", "nota": "Control odontológico." })),
                )
                .await?;
        }
        let hoy = self.hoy;
        for v in visitas.iter().filter(|v| v.fecha > hoy) {
            // Se agendó en los últimos 4 días, y nunca después de ahora.
            let horas = i64::try_from(self.azar.entre(1, 96))?;
            let creada = (ahora - TimeDelta::hours(horas)).min(ahora);
            self.reloj.fijar(creada.with_timezone(&Utc));
            let en_linea = self.en_linea(v);
            let Some((cita, sin_confirmar)) = self.crear_cita(v, en_linea).await? else {
                continue;
            };
            if sin_confirmar && self.azar.prob(0.5) {
                self.reloj
                    .fijar((creada + TimeDelta::minutes(40)).min(ahora).with_timezone(&Utc));
                let _ = self
                    .api
                    .pedir(
                        Method::POST,
                        &format!("/api/v1/agenda/citas/{cita}/confirmar"),
                        None,
                    )
                    .await?;
            }
        }
        self.reloj.restablecer();
        Ok(())
    }

    /// Vacaciones de Daniel sobre citas ya agendadas (deja el aviso para reasignarlas) y dos
    /// solicitudes de habeas data.
    async fn extras(&mut self) -> Resultado<()> {
        self.reloj.restablecer();
        let daniel = self
            .barberos
            .get(3)
            .copied()
            .ok_or_else(|| error("falta Daniel"))?;
        let (desde, hasta) = (dias(self.hoy, 10), dias(self.hoy, 16));
        let (estado, json) = self
            .api
            .pedir(
                Method::POST,
                &format!("/api/v1/barberos/{daniel}/ausencias"),
                Some(json!({ "tipo": "vacaciones", "desde": desde, "hasta": hasta, "observacion": "Vacaciones de fin de año." })),
            )
            .await?;
        if estado != StatusCode::CREATED {
            tracing::warn!(%estado, %json, "vacaciones omitidas");
        }
        let con_id: Vec<i64> = self.clientes.iter().filter_map(|c| c.id).take(2).collect();
        if let [primero, segundo] = con_id[..] {
            self.api
                .exigir(
                    Method::POST,
                    "/api/v1/clientes/solicitudes-titular",
                    Some(json!({ "cliente_id": primero, "tipo": "consulta", "descripcion": "Quiere saber qué datos suyos tenemos y para qué los usamos." })),
                    201,
                )
                .await?;
            let respondida = self
                .api
                .exigir(
                    Method::POST,
                    "/api/v1/clientes/solicitudes-titular",
                    Some(json!({ "cliente_id": segundo, "tipo": "actualizacion", "descripcion": "Cambió de correo electrónico." })),
                    201,
                )
                .await?;
            self.api
                .exigir(
                    Method::PATCH,
                    &format!("/api/v1/clientes/solicitudes-titular/{}", id(&respondida)?),
                    Some(json!({ "respuesta": "Se actualizó el correo en su ficha." })),
                    200,
                )
                .await?;
        }
        Ok(())
    }
}
