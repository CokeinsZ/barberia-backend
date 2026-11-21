//! Cola de correos sobre PostgreSQL real: encolar dentro de una transacción, reservar sin
//! duplicar entre procesos y borrar los secretos al enviar.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use contratos::notificaciones::RespuestaWhatsAppRecibida;
use notificaciones::api::{CorreoSaliente, PlantillaCorreo, PlantillaWhatsApp, WhatsAppSaliente};
use notificaciones::{DependenciasNotificaciones, ModuloNotificaciones};
use nucleo::eventos::{BusEventos, ErrorDeManejador, ManejadorEvento};
use nucleo::{Celular, Correo, Reloj, RelojControlado};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::configuracion::{ConfiguracionSmtp, ConfiguracionWhatsApp, SeguridadSmtp};
use serde_json::{Value, json};

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

const SIMULADO: ConfiguracionWhatsApp = ConfiguracionWhatsApp {
    simulado: true,
    webhook: None,
};

fn modulo(
    base: &BaseDePrueba,
    reloj: Arc<dyn Reloj>,
    smtp: Option<&ConfiguracionSmtp>,
    bus: Arc<BusEventos<UnidadDeTrabajo>>,
) -> ModuloNotificaciones {
    ModuloNotificaciones::con_postgres(DependenciasNotificaciones {
        pool: base.pool.clone(),
        reloj,
        smtp,
        whatsapp: &SIMULADO,
        bus,
    })
    .unwrap()
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
    let modulo = modulo(&base, reloj, None, Arc::new(BusEventos::nuevo()));
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
    let modulo = modulo(&base, reloj.clone(), Some(&smtp), Arc::new(BusEventos::nuevo()));
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

fn recordatorio(celular: &str) -> WhatsAppSaliente {
    WhatsAppSaliente {
        destinatario: Celular::nuevo(celular).unwrap(),
        plantilla: PlantillaWhatsApp::RecordatorioCita {
            nombre: "Andrea".into(),
            servicio: "Corte clásico".into(),
            barbero: "Daniel".into(),
            fecha: "lunes 23 de noviembre".into(),
            hora: "9:00 a. m.".into(),
        },
    }
}

async fn encolar(modulo: &ModuloNotificaciones, base: &BaseDePrueba, mensaje: WhatsAppSaliente) -> i64 {
    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    let id = modulo.buzon().encolar_whatsapp(&mut uow, mensaje).await.unwrap();
    uow.confirmar().await.unwrap();
    id
}

async fn estado(base: &BaseDePrueba, id: i64) -> (String, i16, Option<String>, Value) {
    sqlx::query_as(
        "SELECT estado::text, intentos, id_proveedor, variables FROM notificaciones.mensaje WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&base.pool)
    .await
    .unwrap()
}

/// E2E-NOT-04 a nivel de módulo: el simulador entrega a los números normales y falla con los
/// terminados en 0000; tras el quinto intento el mensaje queda fallido y hay un aviso.
#[tokio::test]
async fn whatsapp_simulado_reintenta_y_avisa_al_agotar_los_intentos() {
    let base = base().await;
    let reloj = Arc::new(RelojControlado::en("2026-11-21T15:00:00Z".parse().unwrap()));
    let modulo = modulo(&base, reloj.clone(), None, Arc::new(BusEventos::nuevo()));
    assert!(modulo.envia_whatsapp());
    let bueno = encolar(&modulo, &base, recordatorio("3001112233")).await;
    let codigo = encolar(
        &modulo,
        &base,
        WhatsAppSaliente {
            destinatario: Celular::nuevo("3002223344").unwrap(),
            plantilla: PlantillaWhatsApp::CodigoVerificacion {
                codigo: "4821".into(),
            },
        },
    )
    .await;
    let malo = encolar(&modulo, &base, recordatorio("3001110000")).await;

    let resumen = modulo.enviar_whatsapp_pendientes().await.unwrap();
    assert_eq!((resumen.enviados, resumen.reintentos), (2, 1));
    let (estado_bueno, _, wamid, _) = estado(&base, bueno).await;
    assert_eq!(estado_bueno, "enviado");
    assert!(wamid.unwrap().starts_with("wamid.simulado."));
    let (_, _, _, variables) = estado(&base, codigo).await;
    assert_eq!(
        variables["codigo"], "4821",
        "el simulador conserva el código para las pruebas"
    );

    // Reintentos a 1, 2, 4 y 8 minutos; el quinto intento falla definitivamente.
    for minutos in [1, 2, 4, 8] {
        reloj.adelantar(TimeDelta::minutes(minutos));
        modulo.enviar_whatsapp_pendientes().await.unwrap();
    }
    let (estado_malo, intentos, _, _) = estado(&base, malo).await;
    assert_eq!((estado_malo.as_str(), intentos), ("fallido", 5));
    let avisos: Vec<(String, String, Value)> =
        sqlx::query_as("SELECT tipo::text, titulo, datos FROM notificaciones.aviso")
            .fetch_all(&base.pool)
            .await
            .unwrap();
    assert_eq!(avisos.len(), 1);
    assert_eq!(avisos[0].0, "mensaje_fallido");
    assert_eq!(
        avisos[0].1,
        "No se pudo entregar el recordatorio de la cita de Andrea"
    );
    assert_eq!(avisos[0].2["mensaje_id"], malo);
    base.eliminar().await;
}

/// Manejador de prueba: anota las respuestas que publica el procesamiento de webhooks.
#[derive(Debug, Default)]
struct Respuestas(Arc<Mutex<Vec<(i64, String)>>>);

#[async_trait]
impl ManejadorEvento<RespuestaWhatsAppRecibida, UnidadDeTrabajo> for Respuestas {
    fn nombre(&self) -> &'static str {
        "respuestas_de_prueba"
    }
    async fn manejar(
        &self,
        _: &mut UnidadDeTrabajo,
        evento: &RespuestaWhatsAppRecibida,
    ) -> Result<(), ErrorDeManejador> {
        self.0
            .lock()
            .unwrap()
            .push((evento.mensaje, evento.carga_boton.clone()));
        Ok(())
    }
}

async fn guardar_evento(base: &BaseDePrueba, carga: Value, recibido_en: DateTime<Utc>) {
    sqlx::query("INSERT INTO notificaciones.evento_webhook (carga, recibido_en) VALUES ($1, $2)")
        .bind(carga)
        .bind(recibido_en)
        .execute(&base.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn procesa_estados_desordenados_fallas_y_respuestas_de_botones() {
    let base = base().await;
    let reloj = Arc::new(RelojControlado::en("2026-11-21T15:00:00Z".parse().unwrap()));
    let bus = Arc::new(BusEventos::<UnidadDeTrabajo>::nuevo());
    let respuestas = Arc::new(Mutex::new(Vec::new()));
    bus.suscribir::<RespuestaWhatsAppRecibida, _>(Respuestas(respuestas.clone()));
    let modulo = modulo(&base, reloj.clone(), None, bus);
    let leido = encolar(&modulo, &base, recordatorio("3001112233")).await;
    let rebotado = encolar(&modulo, &base, recordatorio("3005556677")).await;
    modulo.enviar_whatsapp_pendientes().await.unwrap();
    let wamid = |id: i64| {
        let base = &base;
        async move { estado(base, id).await.2.unwrap() }
    };
    let (wamid_leido, wamid_rebotado) = (wamid(leido).await, wamid(rebotado).await);
    let estados = |pares: &[(&str, &str)]| {
        json!({ "object": "whatsapp_business_account", "entry": [{ "changes": [{ "field": "messages", "value": {
            "statuses": pares.iter().map(|(id, s)| json!({ "id": id, "status": s,
                "errors": if *s == "failed" { json!([{ "code": 131047, "title": "Re-engagement message" }]) } else { json!([]) } }))
                .collect::<Vec<_>>()
        }}]}]})
    };
    // «leído» llega antes que «entregado»: el estado no retrocede.
    guardar_evento(&base, estados(&[(&wamid_leido, "read")]), reloj.ahora()).await;
    guardar_evento(
        &base,
        estados(&[(&wamid_leido, "delivered"), (&wamid_rebotado, "failed")]),
        reloj.ahora() + TimeDelta::seconds(1),
    )
    .await;
    let boton = json!({ "entry": [{ "changes": [{ "value": { "messages": [{
        "type": "button", "context": { "id": wamid_leido }, "button": { "payload": "confirmar", "text": "Confirmo" }
    }]}}]}]});
    guardar_evento(&base, boton, reloj.ahora() + TimeDelta::seconds(2)).await;

    let resumen = modulo.procesar_webhooks().await.unwrap();

    assert_eq!(
        (
            resumen.eventos,
            resumen.estados,
            resumen.respuestas,
            resumen.con_error
        ),
        (3, 2, 1, 0)
    );
    assert_eq!(estado(&base, leido).await.0, "leido");
    assert_eq!(estado(&base, rebotado).await.0, "fallido");
    assert_eq!(*respuestas.lock().unwrap(), [(leido, "confirmar".to_owned())]);
    let (titulo, error): (String, Option<String>) = sqlx::query_as(
        "SELECT a.titulo, m.ultimo_error FROM notificaciones.aviso a, notificaciones.mensaje m WHERE m.id = $1",
    )
    .bind(rebotado)
    .fetch_one(&base.pool)
    .await
    .unwrap();
    assert_eq!(titulo, "No se pudo entregar el recordatorio de la cita de Andrea");
    assert_eq!(error.as_deref(), Some("131047: Re-engagement message"));
    let pendientes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notificaciones.evento_webhook WHERE procesado_en IS NULL")
            .fetch_one(&base.pool)
            .await
            .unwrap();
    assert_eq!(pendientes, 0);
    assert_eq!(
        modulo.procesar_webhooks().await.unwrap().eventos,
        0,
        "ya no queda nada"
    );
    base.eliminar().await;
}
