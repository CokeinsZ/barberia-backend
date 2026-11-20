//! Límite de intentos en memoria con ventana deslizante (RNF-SEG-05). La API corre en un solo
//! proceso, así que no hace falta un almacén compartido.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct LimitadorIntentos {
    maximo: usize,
    ventana: Duration,
    registros: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl LimitadorIntentos {
    pub fn nuevo(maximo: usize, ventana: Duration) -> Self {
        Self {
            maximo,
            ventana,
            registros: Mutex::new(HashMap::new()),
        }
    }

    /// Registra un intento con esa clave (una IP, un correo…). Si ya se alcanzó el máximo en la
    /// ventana, no lo cuenta y devuelve cuánto falta para poder reintentar.
    pub fn intentar(&self, clave: &str) -> Result<(), Duration> {
        self.intentar_en(clave, Instant::now())
    }

    fn intentar_en(&self, clave: &str, ahora: Instant) -> Result<(), Duration> {
        let mut registros = self.registros.lock().unwrap_or_else(|e| e.into_inner());
        if registros.len() > 10_000 {
            let ventana = self.ventana;
            registros.retain(|_, cola| cola.back().is_some_and(|t| ahora.duration_since(*t) < ventana));
        }
        let cola = registros.entry(clave.to_owned()).or_default();
        while cola
            .front()
            .is_some_and(|t| ahora.duration_since(*t) >= self.ventana)
        {
            cola.pop_front();
        }
        if let (true, Some(primero)) = (cola.len() >= self.maximo, cola.front()) {
            return Err(self.ventana.saturating_sub(ahora.duration_since(*primero)));
        }
        cola.push_back(ahora);
        Ok(())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn bloquea_al_superar_el_maximo_y_libera_al_pasar_la_ventana() {
        let limitador = LimitadorIntentos::nuevo(3, Duration::from_secs(60));
        let t0 = Instant::now();
        for _ in 0..3 {
            assert!(limitador.intentar_en("1.2.3.4", t0).is_ok());
        }
        let espera = limitador
            .intentar_en("1.2.3.4", t0 + Duration::from_secs(10))
            .unwrap_err();
        assert_eq!(espera, Duration::from_secs(50));
        assert!(limitador.intentar_en("otra-ip", t0).is_ok());
        assert!(
            limitador
                .intentar_en("1.2.3.4", t0 + Duration::from_secs(61))
                .is_ok()
        );
    }
}
