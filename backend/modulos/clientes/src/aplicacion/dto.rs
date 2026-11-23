use chrono::{DateTime, NaiveDate, Utc};
use nucleo::identificadores::{IdBarbero, IdCliente};
use nucleo::{Dinero, Pagina};
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

/// Cuántos clientes hay en cada segmento, con los umbrales vigentes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ConteoSegmentosDto {
    pub sin_visitas: i64,
    pub nuevo: i64,
    pub recurrente: i64,
    pub frecuente: i64,
    /// «Sin volver»: su última visita fue hace más de `dias_inactividad` días.
    pub inactivo: i64,
}

/// Indicadores de la base de clientes (RF-CLI-04): toda la base vigente (sin anonimizados), sin
/// importar los filtros del listado. «Nuevos del mes» y «vuelven en menos de 60 días» salen de
/// las ventas y están en `GET /reportes/clientes` (mismas reglas que los reportes, RN-21).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct IndicadoresClientesDto {
    pub registrados: i64,
    pub por_segmento: ConteoSegmentosDto,
    pub dias_inactividad: i64,
}

/// Listado (RF-CLI-02) con los indicadores de la base en `meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListadoClientesDto {
    #[serde(flatten)]
    pub pagina: Pagina<ClienteListadoDto>,
    pub meta: IndicadoresClientesDto,
}

/// Umbrales de los segmentos (RF-CFG-08, DA-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ConfiguracionSegmentosDto {
    /// «Nuevo»: de 1 a este número de visitas.
    pub max_visitas_nuevo: i32,
    /// «Recurrente»: hasta este número; más visitas, «Frecuente».
    pub max_visitas_recurrente: i32,
    /// «Sin volver»: última visita hace más de estos días (prevalece sobre los demás).
    pub dias_inactividad: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutorizacionDto {
    pub politica_version_id: i64,
    pub canal: String,
    pub otorgada_en: DateTime<Utc>,
    pub revocada: bool,
}

/// Ficha (RF-CLI-03). Las últimas visitas salen de `GET /ventas?cliente_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClienteFichaDto {
    #[serde(flatten)]
    pub resumen: ClienteListadoDto,
    pub fecha_nacimiento: Option<NaiveDate>,
    pub preferencias: Option<String>,
    pub recordatorios_activos: bool,
    pub cliente_desde: DateTime<Utc>,
    /// Total gastado ÷ visitas (RN-21); `None` sin visitas.
    pub ticket_promedio: Option<Dinero>,
    pub autorizaciones: Vec<AutorizacionDto>,
}

/// Solicitud de un titular de datos (Ley 1581, RNF-LEG-03): consulta, actualización o supresión.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SolicitudTitularDto {
    pub id: i64,
    pub cliente_id: Option<IdCliente>,
    /// `consulta`, `actualizacion` o `supresion`.
    pub tipo: String,
    pub descripcion: String,
    pub recibida_en: DateTime<Utc>,
    /// Plazo legal para responder (Ley 1581): 10 días hábiles las consultas y 15 los reclamos
    /// (actualización o supresión). No descuenta festivos: es una alerta conservadora.
    pub responder_antes_de: NaiveDate,
    pub respondida_en: Option<DateTime<Utc>>,
    pub respondida_por: Option<i64>,
    pub respuesta: Option<String>,
}
