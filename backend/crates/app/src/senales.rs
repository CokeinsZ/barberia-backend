/// Espera Ctrl+C o SIGTERM (lo que envía Docker al detener el contenedor).
pub(crate) async fn apagado() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminar = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut senal) => {
                senal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminar = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminar => {},
    }
    tracing::info!("señal de apagado recibida");
}
