//! Primitivas de seguridad: contraseñas (Argon2id), tokens aleatorios, límite de intentos,
//! firmas de URL (HMAC-SHA256) y cifrado de datos sensibles (AES-256-GCM).

mod cifrado;
mod contrasenas;
mod firmas;
mod limitador;
mod tokens;

pub use cifrado::Cifrador;
pub use contrasenas::{cifrar_contrasena, verificar_contrasena};
pub use firmas::FirmadorUrls;
pub use limitador::LimitadorIntentos;
pub use tokens::{Huella, generar_token, huella_token};

#[derive(Debug, thiserror::Error)]
#[error("Error de seguridad: {0}")]
pub struct ErrorSeguridad(pub String);
