use std::collections::HashSet;
use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::PoliticaDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;

/// Versiones de las políticas legales, de la más reciente a la más antigua, marcando la vigente
/// de cada tipo.
#[derive(Debug)]
pub(crate) struct ListarPoliticas<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ListarPoliticas<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<Vec<PoliticaDto>, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let lista = con_vigencia(&self.p, &mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(lista)
    }
}

/// Todas las versiones, marcando la vigente de cada tipo.
pub(crate) async fn con_vigencia<C: Send>(
    p: &Puertos<C>,
    cx: &mut C,
) -> Result<Vec<PoliticaDto>, ErrorNegocio> {
    let ahora = p.reloj.ahora();
    let guardadas = p.politicas.listar(cx).await?;
    let mut con_vigente = HashSet::new();
    Ok(guardadas
        .into_iter()
        .map(|g| {
            // Ordenadas de la más reciente a la más antigua: la primera que ya empezó rige.
            let vigente = g.vigente_desde <= ahora && con_vigente.insert(g.tipo);
            PoliticaDto {
                id: g.id,
                tipo: g.tipo.codigo(),
                version: g.version,
                url: g.url,
                vigente_desde: g.vigente_desde,
                vigente,
            }
        })
        .collect())
}
