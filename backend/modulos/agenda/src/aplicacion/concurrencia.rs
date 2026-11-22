//! Reintento de las operaciones que chocan con otra simultánea (RNF-INT-01).

use std::future::Future;

use nucleo::ErrorPersistencia;

use super::errores::ErrorAgenda;

/// Intentos de una operación antes de responder que la hora ya no está disponible.
const INTENTOS: u32 = 5;

/// Repite la operación cuando chocó con otra simultánea: un bloqueo mutuo o un fallo de
/// serialización de PostgreSQL, o un barbero asignado automáticamente («Cualquier barbero») que
/// otra persona acaba de ocupar. Cada intento es una transacción nueva que ya ve lo que la otra
/// guardó: toma otro barbero libre o responde `409 conflicto_horario` con alternativas, nunca un
/// error interno. Los bloqueos mutuos casi no ocurren desde que las citas de un mismo barbero se
/// guardan por turnos (`turno_del_barbero` en el repositorio).
pub(crate) async fn con_reintentos<T, F, Fut>(mut operacion: F) -> Result<T, ErrorAgenda>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, ErrorAgenda>>,
{
    let mut intento = 1;
    loop {
        match operacion().await {
            Err(ErrorAgenda::Persistencia(ErrorPersistencia::Concurrencia)) if intento < INTENTOS => {
                tracing::info!(intento, "operación simultánea en la agenda: se reintenta");
                intento += 1;
            }
            Err(ErrorAgenda::Persistencia(ErrorPersistencia::Concurrencia)) => {
                return Err(ErrorAgenda::ConflictoHorario {
                    alternativas: Vec::new(),
                });
            }
            otro => return otro,
        }
    }
}

#[cfg(test)]
mod pruebas {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[tokio::test]
    async fn reintenta_los_choques_y_al_tercero_responde_conflicto() {
        let veces = AtomicU32::new(0);
        let r: Result<(), _> = con_reintentos(|| async {
            veces.fetch_add(1, Ordering::SeqCst);
            Err(ErrorAgenda::Persistencia(ErrorPersistencia::Concurrencia))
        })
        .await;
        assert!(matches!(r, Err(ErrorAgenda::ConflictoHorario { .. })));
        assert_eq!(veces.load(Ordering::SeqCst), INTENTOS);

        let veces = AtomicU32::new(0);
        let r = con_reintentos(|| async {
            if veces.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(ErrorAgenda::Persistencia(ErrorPersistencia::Concurrencia))
            } else {
                Ok(7)
            }
        })
        .await;
        assert_eq!((r.ok(), veces.load(Ordering::SeqCst)), (Some(7), 2));
    }

    #[tokio::test]
    async fn los_demas_errores_no_se_reintentan() {
        let veces = AtomicU32::new(0);
        let r: Result<(), _> = con_reintentos(|| async {
            veces.fetch_add(1, Ordering::SeqCst);
            Err(ErrorAgenda::Persistencia(ErrorPersistencia::Otro("x".into())))
        })
        .await;
        assert!(matches!(
            r,
            Err(ErrorAgenda::Persistencia(ErrorPersistencia::Otro(_)))
        ));
        assert_eq!(veces.load(Ordering::SeqCst), 1);
    }
}
