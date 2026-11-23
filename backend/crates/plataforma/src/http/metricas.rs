//! Métricas de las peticiones HTTP en memoria (RNF-OBS-02): cuántas respondió el proceso por
//! clase de estado y con qué latencia, minuto a minuto en los últimos 15 minutos, desde el
//! arranque y por ruta (la plantilla, nunca la URL con sus datos). Cada proceso de la API lleva
//! las suyas: no necesitan base de datos ni un servicio externo.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;

use super::endurecimiento::RutaAtendida;

/// Límite superior (en milisegundos) de cada cubo de latencia; el último cubo es «más que eso».
const CUBOS_MS: [u64; 10] = [10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000];
/// Minutos que se guardan uno por uno.
const MINUTOS: u64 = 15;
/// Rutas distintas que se cuentan por separado (la API tiene menos de 120); de ahí en adelante
/// todo va a «(otras)»: la memoria no crece con lo que inventen las peticiones.
const MAX_RUTAS: usize = 300;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Contadores {
    /// Respuestas por clase: 1xx, 2xx, 3xx, 4xx y 5xx.
    por_clase: [u64; 5],
    /// Solo las que cuentan para la latencia (sin los PDF, que esperan a Gotenberg).
    cubos: [u64; CUBOS_MS.len() + 1],
}

impl Contadores {
    fn anotar(&mut self, estado: u16, duracion: Duration, con_latencia: bool) {
        let clase = usize::from(estado / 100).clamp(1, 5) - 1;
        self.por_clase[clase] += 1;
        if con_latencia {
            let ms = u64::try_from(duracion.as_millis()).unwrap_or(u64::MAX);
            let cubo = CUBOS_MS
                .iter()
                .position(|tope| ms <= *tope)
                .unwrap_or(CUBOS_MS.len());
            self.cubos[cubo] += 1;
        }
    }

    fn sumar(&mut self, otros: &Contadores) {
        for (a, b) in self.por_clase.iter_mut().zip(otros.por_clase) {
            *a += b;
        }
        for (a, b) in self.cubos.iter_mut().zip(otros.cubos) {
            *a += b;
        }
    }

    /// Percentil `q` (0–1) como el límite superior de su cubo; `None` sin datos.
    fn percentil(&self, q: f64) -> Option<u64> {
        let total: u64 = self.cubos.iter().sum();
        if total == 0 {
            return None;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let objetivo = ((total as f64) * q).ceil().max(1.0) as u64;
        let mut acumulado = 0;
        for (i, n) in self.cubos.iter().enumerate() {
            acumulado += n;
            if acumulado >= objetivo {
                return Some(CUBOS_MS.get(i).copied().unwrap_or(u64::MAX));
            }
        }
        Some(u64::MAX)
    }

    fn resumen(&self) -> ResumenHttp {
        let medidas: u64 = self.cubos.iter().sum();
        ResumenHttp {
            peticiones: self.por_clase.iter().sum(),
            exitosas: self.por_clase[1],
            redirecciones: self.por_clase[2],
            errores_del_cliente: self.por_clase[3],
            errores_del_servidor: self.por_clase[4],
            latencia: (medidas > 0).then(|| Latencia {
                medidas,
                p50_ms: self.percentil(0.50).unwrap_or_default(),
                p95_ms: self.percentil(0.95).unwrap_or_default(),
                p99_ms: self.percentil(0.99).unwrap_or_default(),
            }),
        }
    }
}

/// Latencia aproximada: cada percentil es el límite superior de su cubo (10, 25, 50, 100, 250,
/// 500 ms, 1, 2,5, 5 y 10 s); `18446744073709551615` es «más de 10 s».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Latencia {
    /// Peticiones medidas (sin los PDF).
    pub medidas: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ResumenHttp {
    pub peticiones: u64,
    pub exitosas: u64,
    pub redirecciones: u64,
    pub errores_del_cliente: u64,
    pub errores_del_servidor: u64,
    pub latencia: Option<Latencia>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResumenRuta {
    /// `GET /api/v1/reportes/dashboard`.
    pub ruta: String,
    #[serde(flatten)]
    pub resumen: ResumenHttp,
}

#[derive(Debug, Default)]
struct Estado {
    /// Anillo de los últimos minutos: (minuto desde el arranque, contadores).
    minutos: [(u64, Contadores); MINUTOS as usize],
    total: Contadores,
    por_ruta: BTreeMap<String, Contadores>,
}

#[derive(Debug)]
pub struct MetricasHttp {
    arranque: Instant,
    estado: Mutex<Estado>,
}

impl Default for MetricasHttp {
    fn default() -> Self {
        Self {
            arranque: Instant::now(),
            estado: Mutex::default(),
        }
    }
}

impl MetricasHttp {
    pub fn nuevas() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn minuto_actual(&self) -> u64 {
        self.arranque.elapsed().as_secs() / 60
    }

    pub fn segundos_en_marcha(&self) -> u64 {
        self.arranque.elapsed().as_secs()
    }

    /// Anota una respuesta. `ruta` es la plantilla (`None` si ninguna ruta la atendió).
    pub fn registrar(&self, ruta: Option<&str>, estado: u16, duracion: Duration, con_latencia: bool) {
        self.registrar_en(self.minuto_actual(), ruta, estado, duracion, con_latencia);
    }

    fn registrar_en(
        &self,
        minuto: u64,
        ruta: Option<&str>,
        estado: u16,
        duracion: Duration,
        con_latencia: bool,
    ) {
        let mut e = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        let ranura = &mut e.minutos[usize::try_from(minuto % MINUTOS).unwrap_or_default()];
        if ranura.0 != minuto {
            *ranura = (minuto, Contadores::default());
        }
        ranura.1.anotar(estado, duracion, con_latencia);
        e.total.anotar(estado, duracion, con_latencia);
        let mut clave = ruta.unwrap_or("(sin ruta)");
        if e.por_ruta.len() >= MAX_RUTAS && !e.por_ruta.contains_key(clave) {
            clave = "(otras)";
        }
        e.por_ruta
            .entry(clave.to_owned())
            .or_default()
            .anotar(estado, duracion, con_latencia);
    }

    /// Lo de los últimos `minutos` (el que corre incluido; hasta 15).
    pub fn ultimos(&self, minutos: u64) -> ResumenHttp {
        self.ultimos_en(self.minuto_actual(), minutos)
    }

    fn ultimos_en(&self, ahora: u64, minutos: u64) -> ResumenHttp {
        let minutos = minutos.clamp(1, MINUTOS);
        let e = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        let mut suma = Contadores::default();
        for (minuto, contadores) in &e.minutos {
            if *minuto <= ahora && ahora - minuto < minutos && contadores.por_clase.iter().sum::<u64>() > 0 {
                suma.sumar(contadores);
            }
        }
        suma.resumen()
    }

    pub fn desde_el_arranque(&self) -> ResumenHttp {
        self.estado
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .total
            .resumen()
    }

    /// Las rutas con errores del servidor y las más lentas (por su p95), hasta `n`.
    pub fn rutas_destacadas(&self, n: usize) -> Vec<ResumenRuta> {
        let e = self.estado.lock().unwrap_or_else(PoisonError::into_inner);
        let mut rutas: Vec<ResumenRuta> = e
            .por_ruta
            .iter()
            .map(|(ruta, c)| ResumenRuta {
                ruta: ruta.clone(),
                resumen: c.resumen(),
            })
            .collect();
        rutas.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.resumen.errores_del_servidor),
                std::cmp::Reverse(r.resumen.latencia.map_or(0, |l| l.p95_ms)),
                r.ruta.clone(),
            )
        });
        rutas.truncate(n);
        rutas
    }
}

/// Mide cada respuesta (estado y latencia). Va por fuera de las rutas: la plantilla de la ruta
/// la deja [`super::anotar_ruta`] en la respuesta. Los PDF no cuentan para la latencia (esperan
/// a Gotenberg hasta decenas de segundos, RNF-REN-04), y las verificaciones de salud no cuentan
/// para nada: son del monitor, y el `503` de una alerta no puede alimentar la alerta de errores.
pub async fn medir(
    State(metricas): State<Arc<MetricasHttp>>,
    peticion: Request,
    siguiente: Next,
) -> Response {
    // Solo los métodos estándar: con cualquier otro (HTTP permite inventarlos) alguien podría
    // crear claves sin fin en las métricas.
    let metodo = match *peticion.method() {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        _ => "OTRO",
    };
    let inicio = Instant::now();
    let respuesta = siguiente.run(peticion).await;
    let atendida = respuesta.extensions().get::<RutaAtendida>();
    if atendida.is_some_and(|r| r.0.starts_with("/api/salud")) {
        return respuesta;
    }
    let ruta = atendida.map(|r| format!("{metodo} {}", r.0));
    let con_latencia = !ruta
        .as_deref()
        .is_some_and(|r| r.ends_with(".pdf") || r.ends_with("/reportes/barberos/{archivo}"));
    metricas.registrar(
        ruta.as_deref(),
        respuesta.status().as_u16(),
        inicio.elapsed(),
        con_latencia,
    );
    respuesta
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn cuenta_por_clase_y_estima_los_percentiles() {
        let m = MetricasHttp::default();
        for i in 0..100 {
            // 90 rápidas (≤ 50 ms), 8 de 300 ms, 2 de 3 s.
            let duracion = match i {
                0..90 => ms(40),
                90..98 => ms(300),
                _ => ms(3_000),
            };
            m.registrar_en(0, Some("GET /api/v1/clientes"), 200, duracion, true);
        }
        m.registrar_en(0, Some("GET /api/v1/clientes"), 404, ms(5), true);
        m.registrar_en(0, Some("POST /api/v1/ventas"), 500, ms(5), true);
        m.registrar_en(0, Some("GET /api/v1/reportes/resumen.pdf"), 200, ms(9_000), false);
        let r = m.ultimos_en(0, 5);
        assert_eq!(
            (
                r.peticiones,
                r.exitosas,
                r.errores_del_cliente,
                r.errores_del_servidor
            ),
            (103, 101, 1, 1)
        );
        let latencia = r.latencia.unwrap();
        assert_eq!(latencia.medidas, 102, "el PDF no cuenta");
        assert_eq!(
            (latencia.p50_ms, latencia.p95_ms, latencia.p99_ms),
            (50, 500, 5_000)
        );
        let destacadas = m.rutas_destacadas(2);
        assert_eq!(
            destacadas[0].ruta, "POST /api/v1/ventas",
            "primero, las que fallaron"
        );
        assert_eq!(destacadas[1].ruta, "GET /api/v1/clientes");
    }

    #[test]
    fn las_rutas_distintas_tienen_tope() {
        let m = MetricasHttp::default();
        for i in 0..(MAX_RUTAS + 50) {
            m.registrar_en(0, Some(&format!("GET /inventada/{i}")), 404, ms(1), true);
        }
        let rutas = m.rutas_destacadas(10_000);
        assert_eq!(rutas.len(), MAX_RUTAS + 1);
        let otras = rutas.iter().find(|r| r.ruta == "(otras)").unwrap();
        assert_eq!(otras.resumen.peticiones, 50);
    }

    #[test]
    fn la_ventana_olvida_los_minutos_viejos() {
        let m = MetricasHttp::default();
        m.registrar_en(0, None, 500, ms(1), true);
        m.registrar_en(3, None, 200, ms(1), true);
        m.registrar_en(10, None, 200, ms(1), true);
        assert_eq!(m.ultimos_en(10, 5).peticiones, 1);
        assert_eq!(m.ultimos_en(10, 15).peticiones, 3);
        assert_eq!(m.ultimos_en(10, 15).errores_del_servidor, 1);
        // El minuto 15 reusa la ranura del 0: lo viejo se descarta.
        m.registrar_en(15, None, 200, ms(1), true);
        assert_eq!(m.ultimos_en(15, 15).errores_del_servidor, 0);
        assert_eq!(m.ultimos_en(15, 15).peticiones, 3);
        assert_eq!(m.desde_el_arranque().peticiones, 4);
        assert_eq!(m.ultimos_en(40, 15).peticiones, 0, "nada reciente");
        assert_eq!(m.ultimos_en(40, 15).latencia, None);
    }
}
