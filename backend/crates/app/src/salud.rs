use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// El proceso está vivo (para el *healthcheck* de Docker).
pub(crate) async fn viva() -> Json<Value> {
    Json(json!({ "estado": "viva" }))
}

/// El proceso puede atender: responde la base de datos.
pub(crate) async fn lista(State(pool): State<PgPool>) -> (StatusCode, Json<Value>) {
    match sqlx::query("SELECT 1").execute(&pool).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "estado": "lista" }))),
        Err(error) => {
            tracing::warn!(error = %error, "la base de datos no responde");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "estado": "sin_base_de_datos" })),
            )
        }
    }
}

/// `diamante salud`: consulta `/api/salud` en el puerto local. La usa el *healthcheck* de
/// Docker, así la imagen no necesita `curl`.
pub(crate) async fn verificar_desde_entorno() -> ExitCode {
    let puerto = std::env::var("DIAMANTE_HTTP_DIRECCION")
        .ok()
        .and_then(|d| d.parse::<SocketAddr>().ok())
        .map_or(8080, |d| d.port());
    match tokio::time::timeout(Duration::from_secs(3), consultar(puerto)).await {
        Ok(Ok(true)) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

async fn consultar(puerto: u16) -> std::io::Result<bool> {
    let mut conexion = TcpStream::connect(("127.0.0.1", puerto)).await?;
    conexion
        .write_all(b"GET /api/salud HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut inicio = [0_u8; 16];
    let leidos = conexion.read(&mut inicio).await?;
    Ok(inicio[..leidos].starts_with(b"HTTP/1.1 200"))
}
