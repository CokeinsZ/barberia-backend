//! Cifrado autenticado (AES-256-GCM) de datos sensibles en reposo (RNF-LEG-04).
//!
//! Formato: `[versión = 1][nonce de 12 bytes][texto cifrado + etiqueta de 16 bytes]`. La
//! versión permite rotar la clave más adelante sin perder los datos antiguos.

use std::fmt;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

use super::ErrorSeguridad;

const VERSION: u8 = 1;
const LARGO_NONCE: usize = 12;

#[derive(Clone)]
pub struct Cifrador {
    cifra: Aes256Gcm,
}

impl fmt::Debug for Cifrador {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cifrador(***)")
    }
}

impl Cifrador {
    pub fn nuevo(clave: &[u8; 32]) -> Self {
        // Una clave de 32 bytes siempre es válida para AES-256.
        #[allow(clippy::expect_used)]
        let cifra = Aes256Gcm::new_from_slice(clave).expect("la clave AES-256 mide 32 bytes");
        Self { cifra }
    }

    pub fn cifrar(&self, texto: &str) -> Result<Vec<u8>, ErrorSeguridad> {
        let mut nonce = [0_u8; LARGO_NONCE];
        getrandom::fill(&mut nonce).map_err(|e| ErrorSeguridad(e.to_string()))?;
        let cifrado = self
            .cifra
            .encrypt(&Nonce::from(nonce), texto.as_bytes())
            .map_err(|_| ErrorSeguridad("no se pudo cifrar".into()))?;
        let mut salida = Vec::with_capacity(1 + LARGO_NONCE + cifrado.len());
        salida.push(VERSION);
        salida.extend_from_slice(&nonce);
        salida.extend_from_slice(&cifrado);
        Ok(salida)
    }

    pub fn descifrar(&self, datos: &[u8]) -> Result<String, ErrorSeguridad> {
        let invalido = || ErrorSeguridad("dato cifrado inválido o clave incorrecta".into());
        let (&version, resto) = datos.split_first().ok_or_else(invalido)?;
        if version != VERSION || resto.len() < LARGO_NONCE {
            return Err(invalido());
        }
        let (nonce, cifrado) = resto.split_at(LARGO_NONCE);
        let nonce: [u8; LARGO_NONCE] = nonce.try_into().map_err(|_| invalido())?;
        let texto = self
            .cifra
            .decrypt(&Nonce::from(nonce), cifrado)
            .map_err(|_| invalido())?;
        String::from_utf8(texto).map_err(|_| invalido())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn cifra_con_nonce_distinto_y_descifra() {
        let cifrador = Cifrador::nuevo(&[7; 32]);
        let a = cifrador.cifrar("O+").unwrap();
        let b = cifrador.cifrar("O+").unwrap();
        assert_ne!(a, b, "el nonce aleatorio hace distinto cada cifrado");
        assert_eq!(cifrador.descifrar(&a).unwrap(), "O+");
    }

    #[test]
    fn detecta_alteraciones_y_claves_equivocadas() {
        let cifrador = Cifrador::nuevo(&[7; 32]);
        let mut dato = cifrador.cifrar("0123456789").unwrap();
        assert!(Cifrador::nuevo(&[8; 32]).descifrar(&dato).is_err());
        let ultimo = dato.len() - 1;
        dato[ultimo] ^= 1;
        assert!(cifrador.descifrar(&dato).is_err());
        assert!(cifrador.descifrar(&[]).is_err());
    }
}
