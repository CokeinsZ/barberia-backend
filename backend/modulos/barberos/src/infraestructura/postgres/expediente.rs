use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use nucleo::ErrorPersistencia;
use nucleo::identificadores::{IdArchivo, IdBarbero, IdDocumento};
use plataforma::base_datos::{UnidadDeTrabajo, a_error_persistencia};

use crate::aplicacion::puertos::{
    DocumentoGuardado, NuevoDocumento, PrivadosGuardados, RepositorioExpediente,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RepositorioExpedientePg;

#[derive(sqlx::FromRow)]
struct FilaDocumento {
    id: i64,
    tipo: String,
    periodo: Option<NaiveDate>,
    archivo_id: i64,
    cargado_en: DateTime<Utc>,
}

impl From<FilaDocumento> for DocumentoGuardado {
    fn from(f: FilaDocumento) -> Self {
        DocumentoGuardado {
            id: IdDocumento(f.id),
            tipo: f.tipo,
            periodo: f.periodo,
            archivo: IdArchivo(f.archivo_id),
            cargado_en: f.cargado_en,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FilaPrivados {
    direccion: Option<String>,
    ciudad: Option<String>,
    grupo_sanguineo_cifrado: Option<Vec<u8>>,
    emergencia_nombre: Option<String>,
    emergencia_parentesco: Option<String>,
    emergencia_celular: Option<String>,
    eps: Option<String>,
    fondo_pensiones: Option<String>,
    arl: Option<String>,
    caja_compensacion: Option<String>,
    medio_pago: Option<String>,
    banco: Option<String>,
    tipo_cuenta: Option<String>,
    numero_cuenta_cifrado: Option<Vec<u8>>,
}

#[async_trait]
impl RepositorioExpediente<UnidadDeTrabajo> for RepositorioExpedientePg {
    async fn documentos(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
    ) -> Result<Vec<DocumentoGuardado>, ErrorPersistencia> {
        let filas: Vec<FilaDocumento> = sqlx::query_as(
            "SELECT id, tipo::text AS tipo, periodo, archivo_id, cargado_en FROM barberos.documento
             WHERE barbero_id = $1 ORDER BY cargado_en DESC, id DESC",
        )
        .bind(barbero.0)
        .fetch_all(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(filas.into_iter().map(DocumentoGuardado::from).collect())
    }

    async fn agregar_documento(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        d: &NuevoDocumento,
    ) -> Result<IdDocumento, ErrorPersistencia> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO barberos.documento (barbero_id, tipo, periodo, archivo_id, cargado_por, cargado_en)
             VALUES ($1, $2::barberos.tipo_adjunto, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(barbero.0)
        .bind(d.tipo.codigo())
        .bind(d.periodo)
        .bind(d.archivo.0)
        .bind(d.por.0)
        .bind(d.ahora)
        .fetch_one(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(IdDocumento(id))
    }

    async fn documento(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        id: IdDocumento,
    ) -> Result<Option<DocumentoGuardado>, ErrorPersistencia> {
        let fila: Option<FilaDocumento> = sqlx::query_as(
            "SELECT id, tipo::text AS tipo, periodo, archivo_id, cargado_en FROM barberos.documento
             WHERE id = $2 AND barbero_id = $1",
        )
        .bind(barbero.0)
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(DocumentoGuardado::from))
    }

    async fn quitar_documento(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        id: IdDocumento,
    ) -> Result<Option<DocumentoGuardado>, ErrorPersistencia> {
        let fila: Option<FilaDocumento> = sqlx::query_as(
            "DELETE FROM barberos.documento WHERE id = $2 AND barbero_id = $1
             RETURNING id, tipo::text AS tipo, periodo, archivo_id, cargado_en",
        )
        .bind(barbero.0)
        .bind(id.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(DocumentoGuardado::from))
    }

    async fn datos_privados(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
    ) -> Result<Option<PrivadosGuardados>, ErrorPersistencia> {
        let fila: Option<FilaPrivados> = sqlx::query_as(
            "SELECT direccion, ciudad, grupo_sanguineo_cifrado, emergencia_nombre, emergencia_parentesco::text AS emergencia_parentesco,
                    emergencia_celular::text AS emergencia_celular, eps, fondo_pensiones, arl, caja_compensacion,
                    medio_pago::text AS medio_pago, banco, tipo_cuenta, numero_cuenta_cifrado
             FROM barberos.datos_privados WHERE barbero_id = $1",
        )
        .bind(barbero.0)
        .fetch_optional(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(fila.map(|f| PrivadosGuardados {
            direccion: f.direccion,
            ciudad: f.ciudad,
            grupo_sanguineo_cifrado: f.grupo_sanguineo_cifrado,
            emergencia_nombre: f.emergencia_nombre,
            emergencia_parentesco: f.emergencia_parentesco,
            emergencia_celular: f.emergencia_celular,
            eps: f.eps,
            fondo_pensiones: f.fondo_pensiones,
            arl: f.arl,
            caja_compensacion: f.caja_compensacion,
            medio_pago: f.medio_pago,
            banco: f.banco,
            tipo_cuenta: f.tipo_cuenta,
            numero_cuenta_cifrado: f.numero_cuenta_cifrado,
        }))
    }

    async fn guardar_privados(
        &self,
        uow: &mut UnidadDeTrabajo,
        barbero: IdBarbero,
        d: &PrivadosGuardados,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        sqlx::query(
            "INSERT INTO barberos.datos_privados
                 (barbero_id, direccion, ciudad, grupo_sanguineo_cifrado, emergencia_nombre, emergencia_parentesco,
                  emergencia_celular, eps, fondo_pensiones, arl, caja_compensacion, medio_pago, banco, tipo_cuenta,
                  numero_cuenta_cifrado, actualizado_en)
             VALUES ($1, $2, $3, $4, $5, $6::barberos.parentesco, $7, $8, $9, $10, $11, $12::barberos.medio_pago, $13, $14, $15, $16)
             ON CONFLICT (barbero_id) DO UPDATE SET
                 direccion = EXCLUDED.direccion, ciudad = EXCLUDED.ciudad,
                 grupo_sanguineo_cifrado = EXCLUDED.grupo_sanguineo_cifrado, emergencia_nombre = EXCLUDED.emergencia_nombre,
                 emergencia_parentesco = EXCLUDED.emergencia_parentesco, emergencia_celular = EXCLUDED.emergencia_celular,
                 eps = EXCLUDED.eps, fondo_pensiones = EXCLUDED.fondo_pensiones, arl = EXCLUDED.arl,
                 caja_compensacion = EXCLUDED.caja_compensacion, medio_pago = EXCLUDED.medio_pago, banco = EXCLUDED.banco,
                 tipo_cuenta = EXCLUDED.tipo_cuenta, numero_cuenta_cifrado = EXCLUDED.numero_cuenta_cifrado,
                 actualizado_en = EXCLUDED.actualizado_en",
        )
        .bind(barbero.0)
        .bind(&d.direccion)
        .bind(&d.ciudad)
        .bind(&d.grupo_sanguineo_cifrado)
        .bind(&d.emergencia_nombre)
        .bind(&d.emergencia_parentesco)
        .bind(&d.emergencia_celular)
        .bind(&d.eps)
        .bind(&d.fondo_pensiones)
        .bind(&d.arl)
        .bind(&d.caja_compensacion)
        .bind(&d.medio_pago)
        .bind(&d.banco)
        .bind(&d.tipo_cuenta)
        .bind(&d.numero_cuenta_cifrado)
        .bind(ahora)
        .execute(uow.conexion())
        .await
        .map_err(a_error_persistencia)?;
        Ok(())
    }
}
