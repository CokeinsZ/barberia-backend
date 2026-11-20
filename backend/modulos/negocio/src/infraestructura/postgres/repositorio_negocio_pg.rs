use async_trait::async_trait;
use nucleo::ErrorPersistencia;
use plataforma::base_datos::a_error_persistencia;
use sqlx::PgPool;

use crate::aplicacion::puertos::RepositorioNegocio;
use crate::dominio::entidades::{DatosNegocio, PropuestaDatosNegocio};

#[derive(Debug)]
pub(crate) struct RepositorioNegocioPg {
    pool: PgPool,
}

impl RepositorioNegocioPg {
    pub(crate) fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct FilaNegocio {
    nombre_comercial: String,
    nit: Option<String>,
    direccion: Option<String>,
    zona_referencia: Option<String>,
    whatsapp: Option<String>,
    instagram: Option<String>,
    comision_centesimas: i32,
}

#[async_trait]
impl RepositorioNegocio for RepositorioNegocioPg {
    async fn obtener(&self) -> Result<DatosNegocio, ErrorPersistencia> {
        let fila: FilaNegocio = sqlx::query_as(
            "SELECT nombre_comercial, nit, direccion, zona_referencia, whatsapp::text AS whatsapp, instagram,
                    (porcentaje_comision_defecto * 100)::int4 AS comision_centesimas
             FROM negocio.negocio
             WHERE id = 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(a_error_persistencia)?;

        DatosNegocio::nuevo(PropuestaDatosNegocio {
            nombre_comercial: fila.nombre_comercial,
            nit: fila.nit,
            direccion: fila.direccion,
            zona_referencia: fila.zona_referencia,
            whatsapp: fila.whatsapp,
            instagram: fila.instagram,
            porcentaje_comision_defecto: f64::from(fila.comision_centesimas) / 100.0,
        })
        .map_err(|e| {
            ErrorPersistencia::Otro(format!("datos del negocio inválidos en la base de datos: {e:?}"))
        })
    }

    async fn guardar(&self, datos: &DatosNegocio) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "UPDATE negocio.negocio
             SET nombre_comercial = $1, nit = $2, direccion = $3, zona_referencia = $4,
                 whatsapp = $5, instagram = $6, porcentaje_comision_defecto = $7::numeric / 100
             WHERE id = 1",
        )
        .bind(datos.nombre_comercial())
        .bind(datos.nit())
        .bind(datos.direccion())
        .bind(datos.zona_referencia())
        .bind(datos.whatsapp().map(|c| c.digitos()))
        .bind(datos.instagram())
        .bind(i32::from(datos.porcentaje_comision_defecto().centesimas()))
        .execute(&self.pool)
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}
