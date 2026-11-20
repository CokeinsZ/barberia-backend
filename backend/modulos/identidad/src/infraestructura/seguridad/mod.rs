//! Contraseñas con Argon2id (parámetros recomendados por OWASP, en `plataforma::seguridad`).

use async_trait::async_trait;

use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::Credenciales;
use crate::dominio::ContrasenaNueva;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CredencialesArgon2;

#[async_trait]
impl Credenciales for CredencialesArgon2 {
    async fn cifrar(&self, contrasena: &ContrasenaNueva) -> Result<String, ErrorIdentidad> {
        Ok(plataforma::seguridad::cifrar_contrasena(contrasena.exponer().to_owned()).await?)
    }

    async fn verificar(&self, contrasena: &str, hash: Option<&str>) -> bool {
        plataforma::seguridad::verificar_contrasena(contrasena.to_owned(), hash.map(str::to_owned)).await
    }
}
