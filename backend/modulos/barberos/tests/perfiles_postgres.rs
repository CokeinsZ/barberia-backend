//! El puerto `PerfilesBarberos` sobre PostgreSQL: nombre del barbero y acceso según su estado
//! del día (retirado o suspendido no entra).
//!
//! Requiere `PRUEBAS_DATABASE_URL` (ver `plataforma::base_datos::pruebas`).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use barberos::ModuloBarberos;
use nucleo::RelojControlado;
use nucleo::identificadores::{IdBarbero, IdUsuario};
use plataforma::base_datos::pruebas::BaseDePrueba;
use plataforma::base_datos::{MIGRACIONES_BASE, OrigenMigraciones, UnidadDeTrabajo};

// `barberos` referencia tablas de `archivos` y `catalogo`.
static MIGRACIONES_ARCHIVOS: sqlx::migrate::Migrator = sqlx::migrate!("../archivos/migraciones");
static MIGRACIONES_CATALOGO: sqlx::migrate::Migrator = sqlx::migrate!("../catalogo/migraciones");

#[tokio::test]
async fn el_acceso_depende_del_estado_del_dia() {
    let base = BaseDePrueba::crear(&[
        OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        },
        OrigenMigraciones {
            modulo: "identidad",
            migraciones: &identidad::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "archivos",
            migraciones: &MIGRACIONES_ARCHIVOS,
        },
        OrigenMigraciones {
            modulo: "catalogo",
            migraciones: &MIGRACIONES_CATALOGO,
        },
        OrigenMigraciones {
            modulo: "barberos",
            migraciones: &barberos::MIGRACIONES,
        },
    ])
    .await;
    sqlx::raw_sql(
        "INSERT INTO identidad.usuario (correo, rol) VALUES ('laura@diamante.co', 'administrador'),
                                                            ('santiago@diamante.co', 'barbero');
         INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos,
                                       fecha_nacimiento, celular, especialidad, fecha_ingreso, tipo_vinculacion,
                                       porcentaje_comision)
         VALUES (2, 'CC', '1020304050', 'Santiago', 'Ruiz', '1995-03-02', '3105559917', 'Fade', '2025-01-15',
                 'prestacion_servicios', 50);",
    )
    .execute(&base.pool)
    .await
    .unwrap();

    // Viernes 20 de noviembre de 2026 a las 11 p. m. en Bogotá (ya es sábado en UTC).
    let reloj = Arc::new(RelojControlado::en("2026-11-21T04:00:00Z".parse().unwrap()));
    let perfiles = ModuloBarberos::perfiles_para_identidad(reloj);
    let consultar = |usuario| {
        let perfiles = perfiles.clone();
        let pool = base.pool.clone();
        async move {
            let mut uow = UnidadDeTrabajo::iniciar(&pool).await.unwrap();
            perfiles.por_usuario(&mut uow, IdUsuario(usuario)).await.unwrap()
        }
    };

    let perfil = consultar(2).await.unwrap();
    assert_eq!(
        (perfil.barbero, perfil.nombre.as_str(), perfil.puede_acceder),
        (IdBarbero(1), "Santiago Ruiz", true)
    );
    assert_eq!(
        consultar(1).await,
        None,
        "una cuenta de administrador no tiene perfil de barbero"
    );

    // Una suspensión que empieza el sábado todavía no aplica el viernes en Bogotá.
    sqlx::query(
        "INSERT INTO barberos.ausencia (barbero_id, tipo, desde, hasta, registrada_por)
         VALUES (1, 'suspension', '2026-11-21', '2026-11-25', 1)",
    )
    .execute(&base.pool)
    .await
    .unwrap();
    assert!(consultar(2).await.unwrap().puede_acceder);
    sqlx::query("UPDATE barberos.ausencia SET desde = '2026-11-20'")
        .execute(&base.pool)
        .await
        .unwrap();
    assert!(!consultar(2).await.unwrap().puede_acceder);

    sqlx::query("UPDATE barberos.ausencia SET anulada_en = now()")
        .execute(&base.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE barberos.barbero SET fecha_retiro = '2026-11-20', motivo_retiro = 'renuncia_voluntaria'",
    )
    .execute(&base.pool)
    .await
    .unwrap();
    assert!(!consultar(2).await.unwrap().puede_acceder);
    base.eliminar().await;
}
