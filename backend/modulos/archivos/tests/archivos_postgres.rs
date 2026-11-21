//! Archivos sobre PostgreSQL real y un directorio real: guardar en la transacción de quien
//! llama, descargar con URL firmada y servir solo lo público en `/media`.
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use archivos::api::{ArchivoNuevo, TipoArchivo, Visibilidad};
use archivos::{DependenciasArchivos, ModuloArchivos};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use nucleo::RelojSistema;
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};
use plataforma::seguridad::FirmadorUrls;
use tower::ServiceExt;

static MIGRACIONES_IDENTIDAD: sqlx::migrate::Migrator = sqlx::migrate!("../identidad/migraciones");

fn png_vertical() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend_from_slice(&300_u32.to_be_bytes());
    bytes.extend_from_slice(&400_u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn nuevo(contenido: Vec<u8>, visibilidad: Visibilidad, carpeta: &'static str) -> ArchivoNuevo {
    ArchivoNuevo {
        campo: "archivo",
        contenido,
        nombre_original: "cédula.pdf".into(),
        visibilidad,
        carpeta,
        tipos_permitidos: TipoArchivo::DOCUMENTOS,
        tamano_maximo: 1024 * 1024,
        exigir_vertical: false,
        subido_por: None,
    }
}

async fn pedir(app: &Router, ruta: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let respuesta = app
        .clone()
        .oneshot(Request::get(ruta).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (partes, cuerpo) = respuesta.into_parts();
    (
        partes.status,
        partes.headers,
        cuerpo.collect().await.unwrap().to_bytes().to_vec(),
    )
}

#[tokio::test]
async fn guardar_descargar_y_servir_lo_publico() {
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
            modulo: "archivos",
            migraciones: &archivos::MIGRACIONES,
        },
    ])
    .await;
    let raiz = std::env::temp_dir().join(format!("diamante-archivos-{}", std::process::id()));
    let modulo = ModuloArchivos::con_postgres(DependenciasArchivos {
        pool: base.pool.clone(),
        reloj: Arc::new(RelojSistema),
        raiz: raiz.clone(),
        firmador: FirmadorUrls::nuevo(b"clave de pruebas de integracion"),
    });
    let fachada = modulo.archivos();

    // Una transacción revertida no deja registro (el contenido queda para la limpieza).
    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    fachada
        .guardar(
            &mut uow,
            nuevo(b"%PDF-1.4 revertido".to_vec(), Visibilidad::Privado, "pruebas"),
        )
        .await
        .unwrap();
    uow.revertir().await.unwrap();

    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    let privado = fachada
        .guardar(
            &mut uow,
            nuevo(b"%PDF-1.4 contrato".to_vec(), Visibilidad::Privado, "pruebas"),
        )
        .await
        .unwrap();
    let publico = fachada
        .guardar(&mut uow, nuevo(png_vertical(), Visibilidad::Publico, "pruebas"))
        .await
        .unwrap();
    uow.confirmar().await.unwrap();
    let registros: i64 = sqlx::query_scalar("SELECT count(*) FROM archivos.archivo")
        .fetch_one(&base.pool)
        .await
        .unwrap();
    assert_eq!(registros, 2);
    assert!(raiz.join(privado.clave.como_str()).exists());

    let app = Router::new()
        .nest("/api/v1", modulo.rutas())
        .merge(modulo.rutas_media());
    let url = fachada.url_firmada(privado.id, Duration::from_secs(300));
    let (estado, cabeceras, cuerpo) = pedir(&app, &url).await;
    assert_eq!(estado, StatusCode::OK);
    assert_eq!(cuerpo, b"%PDF-1.4 contrato");
    assert_eq!(cabeceras[header::CONTENT_TYPE], "application/pdf");
    assert!(
        cabeceras[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert_eq!(cabeceras[header::CACHE_CONTROL], "private, no-store");

    let alterada = url.replace("firma=", "firma=x");
    assert_eq!(pedir(&app, &alterada).await.0, StatusCode::FORBIDDEN);
    let (estado, _, _) = pedir(&app, &format!("/api/v1/archivos/{}?exp=1&firma=abc", privado.id)).await;
    assert_eq!(estado, StatusCode::FORBIDDEN);

    let (estado, cabeceras, _) = pedir(&app, &publico.clave.url_publica()).await;
    assert_eq!(
        (estado, cabeceras[header::CONTENT_TYPE].to_str().unwrap()),
        (StatusCode::OK, "image/png")
    );
    assert_eq!(
        pedir(&app, &format!("/media/{}", privado.clave)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        pedir(&app, "/media/publico/../privado/x.pdf").await.0,
        StatusCode::NOT_FOUND
    );

    // Eliminar: el registro en la transacción y el contenido después de confirmar.
    let mut uow = UnidadDeTrabajo::iniciar(&base.pool).await.unwrap();
    let clave = fachada.eliminar(&mut uow, privado.id).await.unwrap().unwrap();
    uow.confirmar().await.unwrap();
    fachada.borrar_contenido(&clave).await;
    assert!(!raiz.join(clave.como_str()).exists());
    assert_eq!(
        pedir(&app, &fachada.url_firmada(privado.id, Duration::from_secs(60)))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    let _ = std::fs::remove_dir_all(&raiz);
    base.eliminar().await;
}
