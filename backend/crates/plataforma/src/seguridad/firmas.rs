//! Firmas HMAC-SHA256 para URL de corta duración (descarga de archivos privados).

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::ErrorSeguridad;

type HmacSha256 = Hmac<Sha256>;

/// Firma y verifica mensajes cortos. La clave nunca se imprime.
#[derive(Clone)]
pub struct FirmadorUrls {
    clave: Vec<u8>,
}

impl fmt::Debug for FirmadorUrls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FirmadorUrls(***)")
    }
}

impl FirmadorUrls {
    pub fn nuevo(clave: &[u8]) -> Self {
        Self {
            clave: clave.to_vec(),
        }
    }

    /// Clave aleatoria de 256 bits (desarrollo y pruebas): las URL firmadas dejan de servir al
    /// reiniciar el proceso.
    pub fn aleatorio() -> Result<Self, ErrorSeguridad> {
        let mut clave = [0_u8; 32];
        getrandom::fill(&mut clave).map_err(|e| ErrorSeguridad(e.to_string()))?;
        Ok(Self::nuevo(&clave))
    }

    fn mac(&self) -> HmacSha256 {
        // HMAC acepta claves de cualquier longitud: este `expect` no puede fallar.
        #[allow(clippy::expect_used)]
        HmacSha256::new_from_slice(&self.clave).expect("HMAC acepta claves de cualquier longitud")
    }

    /// HMAC-SHA256 en bytes (p. ej. la huella de un código de verificación: sin la clave del
    /// servidor no se puede adivinar el código a partir de la huella).
    pub fn huella(&self, mensaje: &str) -> [u8; 32] {
        let mut mac = self.mac();
        mac.update(mensaje.as_bytes());
        mac.finalize().into_bytes().into()
    }

    /// Firma en base64url sin relleno.
    pub fn firmar(&self, mensaje: &str) -> String {
        let mut mac = self.mac();
        mac.update(mensaje.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Comparación en tiempo constante.
    pub fn verificar(&self, mensaje: &str, firma: &str) -> bool {
        let Ok(bytes) = URL_SAFE_NO_PAD.decode(firma) else {
            return false;
        };
        let mut mac = self.mac();
        mac.update(mensaje.as_bytes());
        mac.verify_slice(&bytes).is_ok()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn verifica_solo_la_firma_correcta_del_mismo_mensaje() {
        let firmador = FirmadorUrls::nuevo(b"clave de prueba");
        let firma = firmador.firmar("archivo:7:1800000000");
        assert!(firmador.verificar("archivo:7:1800000000", &firma));
        assert!(!firmador.verificar("archivo:8:1800000000", &firma));
        assert!(!firmador.verificar("archivo:7:1800000000", "firma-falsa"));
        assert!(!FirmadorUrls::nuevo(b"otra clave").verificar("archivo:7:1800000000", &firma));
        assert_eq!(format!("{firmador:?}"), "FirmadorUrls(***)");
    }
}
