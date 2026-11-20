//! El auditor escribe dentro de la transacción de quien llama (si se revierte, no queda
//! rastro) y `GET /auditoria` filtra y pagina.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use async_trait::async_trait;
use auditoria::ModuloAuditoria;
use auditoria::api::EntradaAuditoria;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use nucleo::identificadores::IdUsuario;
use nucleo::{Actor, RelojControlado};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::http::{ErrorHttp, Sesiones, ValidadorSesion};
use serde_json::{Value, json};
use tower::ServiceExt;

static MIGRACIONES_IDENTIDAD: sqlx::migrate::Migrator = sqlx::migrate!("../identidad/migraciones");

#[derive(Debug)]
struct SoloAdmin;

#[async_trait]
impl ValidadorSesion for SoloAdmin {
    async fn validar(&self, token: &str) -> Result<Option<Actor>, ErrorHttp> {
        Ok((token == "admin").then_some(Actor::Administrador {
            usuario: IdUsuario(1),
        }))
    }
}

#[tokio::test]
async fn registra_en_la_transaccion_y_consulta_con_filtros() {
    let base = BaseDePrueba::crear(&[
        OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        },
        OrigenMigraciones {
            modulo: "identidad",
            migraciones: &MIGRACIONES_IDENTIDAD,
        },
        OrigenMigraciones {
            modulo: "auditoria",
            migraciones: &auditoria::MIGRACIONES,
        },
    ])
    .await;
    sqlx::query("INSERT INTO identidad.usuario (correo, rol) VALUES ('laura@diamante.co', 'administrador')")
        .execute(&base.pool)
        .await
        .unwrap();
    let reloj = Arc::new(RelojControlado::en("2026-11-20T15:00:00Z".parse().unwrap()));
    let modulo = ModuloAuditoria::con_postgres(base.pool.clone(), reloj);
    let auditor = modulo.auditor();
    let laura = Actor::Administrador {
        usuario: IdUsuario(1),
    };

    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    auditor
        .registrar(
            &mut uow,
            EntradaAuditoria::nueva(&laura, "negocio", "revertida", "negocio", 1),
        )
        .await
        .unwrap();
    uow.revertir().await.unwrap();

    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    auditor
        .registrar(
            &mut uow,
            EntradaAuditoria::nueva(&laura, "negocio", "actualizar_datos", "negocio", 1)
                .antes(&json!({ "nit": null }))
                .despues(&json!({ "nit": "900" })),
        )
        .await
        .unwrap();
    auditor
        .registrar(
            &mut uow,
            EntradaAuditoria::nueva(&Actor::Sistema, "identidad", "bloquear_cuenta", "usuario", 1),
        )
        .await
        .unwrap();
    uow.confirmar().await.unwrap();

    let enrutador = Router::new()
        .nest("/api/v1", modulo.rutas())
        .layer(Extension(Sesiones(Arc::new(SoloAdmin))));
    let pedir = |ruta: &str| {
        let peticion = Request::get(ruta)
            .header(header::COOKIE, "__Host-sesion=admin")
            .body(Body::empty())
            .unwrap();
        let enrutador = enrutador.clone();
        async move {
            let respuesta = enrutador.oneshot(peticion).await.unwrap();
            let estado = respuesta.status();
            let bytes = respuesta.into_body().collect().await.unwrap().to_bytes();
            (estado, serde_json::from_slice::<Value>(&bytes).unwrap())
        }
    };

    let (estado, pagina) = pedir("/api/v1/auditoria").await;
    assert_eq!(estado, StatusCode::OK);
    assert_eq!(pagina["total"], 2, "la entrada revertida no quedó");
    assert_eq!(pagina["datos"][0]["actor"], "sistema");

    let (_, pagina) = pedir("/api/v1/auditoria?entidad=negocio&desde=2026-11-20&hasta=2026-11-20").await;
    assert_eq!(pagina["total"], 1);
    assert_eq!(pagina["datos"][0]["usuario_id"], 1);
    assert_eq!(pagina["datos"][0]["despues"]["nit"], "900");

    let (_, pagina) = pedir("/api/v1/auditoria?desde=2026-11-21").await;
    assert_eq!(pagina["total"], 0);
    let (estado, _) = pedir("/api/v1/auditoria?campo_raro=1").await;
    assert_eq!(estado, StatusCode::BAD_REQUEST);
    base.eliminar().await;
}
