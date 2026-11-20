use chrono::{DateTime, Utc};
use nucleo::identificadores::IdUsuario;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegistroAuditoriaDto {
    pub id: i64,
    pub ocurrido_en: DateTime<Utc>,
    pub actor: String,
    pub usuario_id: Option<IdUsuario>,
    pub modulo: String,
    pub accion: String,
    pub entidad: String,
    pub entidad_id: String,
    pub antes: Option<Value>,
    pub despues: Option<Value>,
}
