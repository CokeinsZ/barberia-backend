//! Primitivas de seguridad: contraseñas (Argon2id), tokens aleatorios y límite de intentos.

mod contrasenas;
mod limitador;
mod tokens;

pub use contrasenas::{cifrar_contrasena, verificar_contrasena};
pub use limitador::LimitadorIntentos;
pub use tokens::{Huella, generar_token, huella_token};

#[derive(Debug, thiserror::Error)]
#[error("Error de seguridad: {0}")]
pub struct ErrorSeguridad(pub String);
