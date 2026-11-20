use std::sync::OnceLock;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

use super::ErrorSeguridad;

/// Cifra una contraseña con Argon2id y los parámetros por defecto de la librería
/// (m = 19 MiB, t = 2, p = 1, la recomendación de OWASP). Es costoso a propósito, así que
/// se ejecuta fuera de los hilos asíncronos.
pub async fn cifrar_contrasena(contrasena: String) -> Result<String, ErrorSeguridad> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(contrasena.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| ErrorSeguridad(e.to_string()))
    })
    .await
    .map_err(|e| ErrorSeguridad(e.to_string()))?
}

/// Verifica una contraseña. Si no hay hash (la cuenta no existe o no está activada), se
/// verifica igual contra un hash ficticio, para que el tiempo de respuesta no revele si la
/// cuenta existe.
pub async fn verificar_contrasena(contrasena: String, hash: Option<String>) -> bool {
    tokio::task::spawn_blocking(move || {
        let existe = hash.is_some();
        let hash = hash.unwrap_or_else(|| hash_ficticio().to_owned());
        let coincide = PasswordHash::new(&hash)
            .map(|h| {
                Argon2::default()
                    .verify_password(contrasena.as_bytes(), &h)
                    .is_ok()
            })
            .unwrap_or(false);
        existe && coincide
    })
    .await
    .unwrap_or(false)
}

fn hash_ficticio() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        Argon2::default()
            .hash_password(b"contrasena-ficticia-para-igualar-tiempos")
            .map(|h| h.to_string())
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[tokio::test]
    async fn cifra_y_verifica() {
        let hash = cifrar_contrasena("una clave larga y segura".into())
            .await
            .unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verificar_contrasena("una clave larga y segura".into(), Some(hash.clone())).await);
        assert!(!verificar_contrasena("otra clave".into(), Some(hash)).await);
    }

    #[tokio::test]
    async fn sin_hash_nunca_coincide() {
        assert!(!verificar_contrasena("contrasena-ficticia-para-igualar-tiempos".into(), None).await);
    }
}
