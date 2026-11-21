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

/// Firma de un webhook al estilo de Meta: `sha256=<hex>` con el HMAC-SHA256 del cuerpo exacto
/// recibido y el secreto de la app (encabezado `X-Hub-Signature-256`).
pub fn firma_webhook(secreto: &[u8], cuerpo: &[u8]) -> String {
    let mut mac = mac_con(secreto);
    mac.update(cuerpo);
    let bytes = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(7 + 64);
    hex.push_str("sha256=");
    for byte in bytes {
        hex.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        hex.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
    }
    hex
}

/// Verifica `X-Hub-Signature-256` en tiempo constante (RNF-SEG-10).
pub fn verificar_firma_webhook(secreto: &[u8], cuerpo: &[u8], encabezado: &str) -> bool {
    let Some(hex) = encabezado.trim().strip_prefix("sha256=") else {
        return false;
    };
    let Some(firma) = decodificar_hex(hex) else {
        return false;
    };
    let mut mac = mac_con(secreto);
    mac.update(cuerpo);
    mac.verify_slice(&firma).is_ok()
}

/// Compara dos secretos sin revelar por el tiempo cuántos bytes coinciden.
pub fn iguales_en_tiempo_constante(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .fold(0_u8, |acumulado, (x, y)| acumulado | (x ^ y))
            == 0
}

fn mac_con(clave: &[u8]) -> HmacSha256 {
    // HMAC acepta claves de cualquier longitud: este `expect` no puede fallar.
    #[allow(clippy::expect_used)]
    HmacSha256::new_from_slice(clave).expect("HMAC acepta claves de cualquier longitud")
}

fn decodificar_hex(texto: &str) -> Option<Vec<u8>> {
    if !texto.len().is_multiple_of(2) {
        return None;
    }
    let digito = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    texto
        .as_bytes()
        .chunks(2)
        .map(|par| Some(digito(par[0])? << 4 | digito(par[1])?))
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn firma_y_verifica_webhooks_como_meta() {
        // Vector calculado con: printf '{"a":1}' | openssl dgst -sha256 -hmac secreto
        let firma = firma_webhook(b"secreto", br#"{"a":1}"#);
        assert_eq!(
            firma,
            "sha256=87c6bf752c360faf8b89e2d9ed15dcc45e947f9ecdf9c4b637c8fa62d691c598"
        );
        assert!(verificar_firma_webhook(b"secreto", br#"{"a":1}"#, &firma));
        assert!(verificar_firma_webhook(
            b"secreto",
            br#"{"a":1}"#,
            &firma.to_uppercase().replace("SHA256=", "sha256=")
        ));
        assert!(!verificar_firma_webhook(b"otro", br#"{"a":1}"#, &firma));
        assert!(!verificar_firma_webhook(b"secreto", br#"{"a":2}"#, &firma));
        assert!(!verificar_firma_webhook(b"secreto", br#"{"a":1}"#, "sha256=zz"));
        assert!(!verificar_firma_webhook(b"secreto", br#"{"a":1}"#, ""));
        assert!(iguales_en_tiempo_constante(b"abc", b"abc"));
        assert!(!iguales_en_tiempo_constante(b"abc", b"abd"));
        assert!(!iguales_en_tiempo_constante(b"abc", b"ab"));
    }

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
