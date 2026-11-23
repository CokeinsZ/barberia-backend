//! Binario único del backend. El mismo ejecutable cumple varios papeles en Docker Compose:
//!
//! - `diamante migrar`: aplica las migraciones de todos los módulos y termina.
//! - `diamante api`: servidor HTTP.
//! - `diamante worker`: tareas en segundo plano (correos, WhatsApp, webhooks, recordatorios y
//!   mantenimiento).
//! - `diamante salud`: verificación de salud para Docker (sin `curl` en la imagen).
//! - `diamante crear-administrador <correo> <nombre>`: invita al primer administrador de una
//!   instalación nueva y muestra el enlace de activación.
//! - `diamante enlace-acceso <correo>`: imprime un enlace de activación (cuenta sin activar) o
//!   de recuperación (cuenta activada), sin enviar correo. Queda en la auditoría.

mod adaptadores;
mod administracion;
mod composicion;
mod enrutador;
mod migraciones;
mod pruebas_e2e;
mod salud;
mod senales;
mod sistema;
mod tareas;
mod worker;

#[cfg(test)]
mod pruebas_autorizacion;
#[cfg(test)]
mod pruebas_concurrencia;
#[cfg(test)]
mod pruebas_entrega2;
#[cfg(test)]
mod pruebas_postgres;
#[cfg(test)]
mod pruebas_reportes;
#[cfg(test)]
mod pruebas_robustez;
#[cfg(test)]
mod pruebas_whatsapp;

use std::net::SocketAddr;
use std::process::ExitCode;

use plataforma::configuracion::Configuracion;

type ErrorDeArranque = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> ExitCode {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let comando = argumentos.first().cloned().unwrap_or_default();
    if comando == "salud" {
        return salud::verificar_desde_entorno().await;
    }
    // Las migraciones solo necesitan la base: no exigen la URL pública ni las claves.
    if comando == "migrar" {
        let (url, formato) = match Configuracion::para_migraciones(|clave| {
            std::env::var(clave).ok().filter(|v| !v.trim().is_empty())
        }) {
            Ok(c) => c,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(78); // EX_CONFIG
            }
        };
        plataforma::telemetria::iniciar(formato);
        return match migraciones::ejecutar(&url).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(error = %error, "las migraciones fallaron");
                ExitCode::FAILURE
            }
        };
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
        "enlace-acceso" => match &argumentos[1..] {
            [correo] => administracion::enlace_acceso(&configuracion, correo).await,
            _ => {
                eprintln!("Uso: diamante enlace-acceso <correo>");
                return ExitCode::from(2);
            }
        },
        "crear-administrador" => match &argumentos[1..] {
            [correo, nombre @ ..] if !nombre.is_empty() => {
                administracion::crear_administrador(&configuracion, correo, &nombre.join(" ")).await
            }
            _ => {
                eprintln!("Uso: diamante crear-administrador <correo> <nombre completo>");
                return ExitCode::from(2);
            }
        },
        _ => {
            eprintln!("Uso: diamante <api|worker|migrar|salud|crear-administrador|enlace-acceso>");
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
    if configuracion.worker_en_api {
        // Ambiente E2E: las tareas del worker corren aquí, con el mismo reloj de la API.
        worker::informar_canales(&composicion);
        tracing::info!("la API también ejecuta las tareas del worker (DIAMANTE_WORKER_EN_API)");
        tokio::spawn(worker::bucle(composicion.clone(), std::future::pending()));
    }
    let escucha = tokio::net::TcpListener::bind(configuracion.direccion_http).await?;
    tracing::info!(direccion = %configuracion.direccion_http, entorno = ?configuracion.entorno, "API escuchando");
    // La IP de la conexión alimenta los límites de intentos (o `X-Forwarded-For` tras el proxy).
    axum::serve(
        escucha,
        aplicacion.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(senales::apagado())
    .await?;
    tracing::info!("API detenida");
    Ok(())
}
