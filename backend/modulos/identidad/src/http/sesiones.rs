use std::sync::Arc;

use async_trait::async_trait;
use nucleo::Actor;
use plataforma::http::{ErrorHttp, ValidadorSesion};

use crate::aplicacion::consultas::ValidarSesion;

/// Adapta el caso de uso [`ValidarSesion`] al puerto que usan los extractores HTTP de toda la
/// API (`plataforma::http::ActorAutenticado`).
#[derive(Debug)]
pub(crate) struct ValidadorIdentidad<C: Send>(pub(crate) Arc<ValidarSesion<C>>);

#[async_trait]
impl<C: Send + 'static> ValidadorSesion for ValidadorIdentidad<C> {
    async fn validar(&self, token: &str) -> Result<Option<Actor>, ErrorHttp> {
        Ok(self.0.ejecutar(token).await?)
    }
}
