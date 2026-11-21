//! `GET /api/v1/_pruebas/mensajes?destinatario=` (solo con `DIAMANTE_PRUEBAS_E2E=true`): los
//! mensajes que el WhatsApp simulado «envió», p. ej. para leer el código de verificación.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use plataforma::http::{ErrorHttp, Parametros};
use serde::Deserialize;
use sqlx::PgPool;

use crate::infraestructura::postgres::{MensajeDePrueba, mensajes_recientes};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filtro {
    destinatario: String,
}

pub(crate) fn rutas_de_pruebas(pool: PgPool) -> Router {
    Router::new()
        .route("/_pruebas/mensajes", get(mensajes))
        .with_state(pool)
}

async fn mensajes(
    State(pool): State<PgPool>,
    Parametros(f): Parametros<Filtro>,
) -> Result<Json<Vec<MensajeDePrueba>>, ErrorHttp> {
    let destinatario: String = f.destinatario.chars().filter(|c| !c.is_whitespace()).collect();
    mensajes_recientes(&pool, &destinatario)
        .await
        .map(Json)
        .map_err(|_| ErrorHttp::interno())
}
