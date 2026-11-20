//! Binario único del backend. El mismo ejecutable cumple tres papeles en Docker Compose:
//!
//! - `diamante migrar`: aplica las migraciones de todos los módulos y termina.
//! - `diamante api`: servidor HTTP.
//! - `diamante worker`: tareas en segundo plano.
//! - `diamante salud`: verificación de salud para Docker (sin `curl` en la imagen).

mod composicion;
mod enrutador;
mod migraciones;
mod salud;
mod senales;
mod worker;

use std::process::ExitCode;

use plataforma::configuracion::Configuracion;

type ErrorDeArranque = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> ExitCode {
    let comando = std::env::args().nth(1).unwrap_or_default();
    if comando == "salud" {
        return salud::verificar_desde_entorno().await;
    }

    let configuracion = match Configuracion::desde_entorno() {
        Ok(configuracion) => configuracion,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(78); // EX_CONFIG
        }
    };
    plataforma::telemetria::iniciar(configuracion.formato_logs);

    let resultado = match comando.as_str() {
        "api" => api(&configuracion).await,
        "worker" => worker::ejecutar(&configuracion).await,
        "migrar" => migraciones::ejecutar(&configuracion).await,
        _ => {
            eprintln!("Uso: diamante <api|worker|migrar|salud>");
            return ExitCode::from(2);
        }
    };
    match resultado {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, comando, "el proceso terminó con error");
            ExitCode::FAILURE
        }
    }
}

async fn api(configuracion: &Configuracion) -> Result<(), ErrorDeArranque> {
    let composicion = composicion::Composicion::construir(configuracion).await?;
    let aplicacion = enrutador::construir(&composicion);
    let escucha = tokio::net::TcpListener::bind(configuracion.direccion_http).await?;
    tracing::info!(direccion = %configuracion.direccion_http, entorno = ?configuracion.entorno, "API escuchando");
    axum::serve(escucha, aplicacion)
        .with_graceful_shutdown(senales::apagado())
        .await?;
    tracing::info!("API detenida");
    Ok(())
}
