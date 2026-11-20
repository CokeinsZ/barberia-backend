//! Logs estructurados con `tracing`. El nivel se ajusta con `RUST_LOG`.

use tracing_subscriber::EnvFilter;

use crate::configuracion::FormatoLogs;

/// Inicia el registro de logs. Es seguro llamarla más de una vez (las siguientes no hacen nada).
pub fn iniciar(formato: FormatoLogs) {
    let filtro = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    let constructor = tracing_subscriber::fmt()
        .with_env_filter(filtro)
        .with_target(true);
    let _ = match formato {
        FormatoLogs::Json => constructor.json().flatten_event(true).try_init(),
        FormatoLogs::Texto => constructor.compact().try_init(),
    };
}
