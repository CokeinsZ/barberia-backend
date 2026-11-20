use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use super::ErrorSeguridad;

/// Huella SHA-256 de un token. En la base de datos se guarda la huella, nunca el token.
pub type Huella = [u8; 32];

/// Genera un token aleatorio de 256 bits en Base64 URL (43 caracteres) y su huella.
pub fn generar_token() -> Result<(String, Huella), ErrorSeguridad> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| ErrorSeguridad(e.to_string()))?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let huella = huella_token(&token);
    Ok((token, huella))
}

pub fn huella_token(token: &str) -> Huella {
    let mut huella = [0_u8; 32];
    huella.copy_from_slice(&Sha256::digest(token.as_bytes()));
    huella
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_tokens_son_unicos_y_su_huella_es_estable() {
        let (a, huella_a) = generar_token().unwrap();
        let (b, _) = generar_token().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(huella_token(&a), huella_a);
    }
}
