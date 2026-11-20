//! Cola de correos sobre PostgreSQL real: encolar dentro de una transacción, reservar sin
//! duplicar entre procesos y borrar los secretos al enviar.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use notificaciones::ModuloNotificaciones;
use notificaciones::api::{CorreoSaliente, PlantillaCorreo};
use nucleo::{Correo, RelojControlado};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::configuracion::{ConfiguracionSmtp, SeguridadSmtp};
use serde_json::Value;

/// `notificaciones.aviso` referencia a `identidad.usuario`.
static MIGRACIONES_IDENTIDAD: sqlx::migrate::Migrator = sqlx::migrate!("../identidad/migraciones");

async fn base() -> BaseDePrueba {
    BaseDePrueba::crear(&[
        OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        },
        OrigenMigraciones {
            modulo: "identidad",
            migraciones: &MIGRACIONES_IDENTIDAD,
        },
        OrigenMigraciones {
            modulo: "notificaciones",
            migraciones: &notificaciones::MIGRACIONES,
        },
    ])
    .await
}

fn correo(destinatario: &str) -> CorreoSaliente {
    CorreoSaliente {
        destinatario: Correo::nuevo(destinatario).unwrap(),
        plantilla: PlantillaCorreo::RecuperacionContrasena {
            nombre: "Laura".into(),
            enlace: "https://diamante.co/acceso/restablecer?token=secreto".into(),
        },
    }
}

#[tokio::test]
async fn encola_dentro_de_la_transaccion_y_no_si_se_revierte() {
    let base = base().await;
    let reloj = Arc::new(RelojControlado::en("2026-11-20T15:00:00Z".parse().unwrap()));
    let modulo = ModuloNotificaciones::con_postgres(base.pool.clone(), reloj, None).unwrap();
    let buzon = modulo.buzon();

    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    buzon
        .encolar_correo(&mut uow, correo("revertido@diamante.co"))
        .await
        .unwrap();
    uow.revertir().await.unwrap();

    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    buzon
        .encolar_correo(&mut uow, correo("laura@diamante.co"))
        .await
        .unwrap();
    uow.confirmar().await.unwrap();

    let filas: Vec<(String, String, Value, bool, String)> = sqlx::query_as(
        "SELECT destinatario, plantilla, variables, contiene_secreto, estado::text FROM notificaciones.mensaje",
    )
    .fetch_all(&base.pool)
    .await
    .unwrap();
    assert_eq!(filas.len(), 1);
    let (destinatario, plantilla, variables, secreto, estado) = &filas[0];
    assert_eq!(destinatario, "laura@diamante.co");
    assert_eq!(plantilla, "recuperacion_contrasena");
    assert_eq!(variables["nombre"], "Laura");
    assert!(*secreto);
    assert_eq!(estado, "pendiente");

    // Sin SMTP, el worker no hace nada y el mensaje sigue pendiente.
    assert_eq!(
        modulo.enviar_correos_pendientes().await.unwrap(),
        Default::default()
    );
    base.eliminar().await;
}

/// Un servidor SMTP que rechaza la conexión: el mensaje queda para reintento, reservado
/// hacia el futuro y con el error guardado.
#[tokio::test]
async fn un_fallo_temporal_deja_el_mensaje_para_reintento() {
    let base = base().await;
    let reloj = Arc::new(RelojControlado::en("2026-11-20T15:00:00Z".parse().unwrap()));
    let smtp = ConfiguracionSmtp {
        host: "127.0.0.1".into(),
        puerto: 9, // puerto «discard»: nadie escucha
        seguridad: SeguridadSmtp::Ninguna,
        usuario: None,
        clave: None,
        remitente: "Diamante Barbería <no-responder@diamante.co>".into(),
    };
    let modulo = ModuloNotificaciones::con_postgres(base.pool.clone(), reloj.clone(), Some(&smtp)).unwrap();
    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    modulo
        .buzon()
        .encolar_correo(&mut uow, correo("laura@diamante.co"))
        .await
        .unwrap();
    uow.confirmar().await.unwrap();

    let resumen = modulo.enviar_correos_pendientes().await.unwrap();
    assert_eq!(
        (resumen.enviados, resumen.reintentos, resumen.fallidos),
        (0, 1, 0)
    );

    let (estado, intentos, error, programado): (String, i16, Option<String>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT estado::text, intentos, ultimo_error, programado_para FROM notificaciones.mensaje",
        )
        .fetch_one(&base.pool)
        .await
        .unwrap();
    assert_eq!(estado, "pendiente");
    assert_eq!(intentos, 1);
    assert!(error.is_some());
    assert_eq!(
        programado,
        "2026-11-20T15:01:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
    );

    // Antes de la hora del reintento no se vuelve a tomar.
    assert_eq!(
        modulo.enviar_correos_pendientes().await.unwrap(),
        Default::default()
    );
    base.eliminar().await;
}
