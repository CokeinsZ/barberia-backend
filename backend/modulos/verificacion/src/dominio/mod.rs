//! Reglas del código de verificación (RNF-SEG-06).

use chrono::TimeDelta;

pub(crate) const VIGENCIA_CODIGO: TimeDelta = TimeDelta::minutes(10);
pub(crate) const VIGENCIA_COMPROBANTE: TimeDelta = TimeDelta::minutes(30);
pub(crate) const ESPERA_REENVIO: TimeDelta = TimeDelta::seconds(60);

/// Código de 4 dígitos uniforme (muestreo por rechazo para no sesgar los primeros valores).
pub(crate) fn generar_codigo() -> Result<String, String> {
    loop {
        let mut bytes = [0_u8; 2];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        let valor = u16::from_be_bytes(bytes);
        if valor < 60_000 {
            return Ok(format!("{:04}", valor % 10_000));
        }
    }
}

/// Mensaje que se firma: el código solo vale para ese celular.
pub(crate) fn mensaje_codigo(celular: &str, codigo: &str) -> String {
    format!("verificacion:{celular}:{codigo}")
}

/// Comparación en tiempo constante.
pub(crate) fn iguales(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_codigos_tienen_cuatro_digitos() {
        for _ in 0..200 {
            let codigo = generar_codigo().unwrap();
            assert_eq!(codigo.len(), 4);
            assert!(codigo.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn compara_sin_atajos() {
        assert!(iguales(b"abcd", b"abcd"));
        assert!(!iguales(b"abcd", b"abce"));
        assert!(!iguales(b"abc", b"abcd"));
    }
}
