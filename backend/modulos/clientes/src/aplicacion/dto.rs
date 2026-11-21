use chrono::{DateTime, NaiveDate, Utc};
use nucleo::Dinero;
use nucleo::identificadores::{IdBarbero, IdCliente};
use serde::Serialize;

/// Resultado de la búsqueda rápida de la agenda y la venta (máximo 8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteRapidoDto {
    pub id: IdCliente,
    pub nombre: String,
    pub celular: String,
    pub segmento: &'static str,
}

/// Fila del listado (RF-CLI-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteListadoDto {
    pub id: IdCliente,
    pub nombre: String,
    pub celular: String,
    pub correo: Option<String>,
    pub visitas: i32,
    pub total_gastado: Dinero,
    pub ultima_visita: Option<NaiveDate>,
    pub inasistencias: i32,
    pub barbero_habitual_id: Option<IdBarbero>,
    /// `sin_visitas`, `nuevo`, `recurrente`, `frecuente` o `inactivo` (sin volver).
    pub segmento: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutorizacionDto {
    pub politica_version_id: i64,
    pub canal: String,
    pub otorgada_en: DateTime<Utc>,
    pub revocada: bool,
}

/// Ficha (RF-CLI-03). Las últimas visitas saldrán de las ventas (fase 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteFichaDto {
    #[serde(flatten)]
    pub resumen: ClienteListadoDto,
    pub fecha_nacimiento: Option<NaiveDate>,
    pub preferencias: Option<String>,
    pub recordatorios_activos: bool,
    pub cliente_desde: DateTime<Utc>,
    pub autorizaciones: Vec<AutorizacionDto>,
}
