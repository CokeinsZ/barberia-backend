//! Interpretación de los webhooks de WhatsApp Cloud API: estados de entrega de los mensajes
//! enviados y respuestas a los botones de las plantillas.

use serde::Deserialize;
use serde_json::Value;

use super::whatsapp::{CANCELAR, CONFIRMAR};

/// Estado de entrega que informa Meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EstadoEntrega {
    Enviado,
    Entregado,
    Leido,
    Fallido,
}

impl EstadoEntrega {
    pub(crate) fn codigo(self) -> &'static str {
        match self {
            EstadoEntrega::Enviado => "enviado",
            EstadoEntrega::Entregado => "entregado",
            EstadoEntrega::Leido => "leido",
            EstadoEntrega::Fallido => "fallido",
        }
    }
}

/// Los estados solo avanzan (pendiente → enviado → entregado → leído). Meta puede entregar los
/// avisos desordenados: un «entregado» que llega después de «leído» no hace retroceder el
/// mensaje. «Fallido» aplica mientras no se haya leído; `cancelado` y `fallido` son finales.
pub(crate) fn puede_pasar(actual: &str, nuevo: EstadoEntrega) -> bool {
    let rango = |estado: &str| match estado {
        "pendiente" => Some(0),
        "enviado" => Some(1),
        "entregado" => Some(2),
        "leido" => Some(3),
        _ => None,
    };
    let Some(desde) = rango(actual) else {
        return false;
    };
    match nuevo {
        EstadoEntrega::Fallido => desde < 3,
        otro => rango(otro.codigo()).is_some_and(|hasta| hasta > desde),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CambioDeEstado {
    /// Id del mensaje en Meta (wamid).
    pub(crate) id_proveedor: String,
    pub(crate) estado: EstadoEntrega,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RespuestaDeBoton {
    /// Mensaje nuestro al que responde (wamid del contexto).
    pub(crate) id_original: String,
    /// `confirmar` o `cancelar`.
    pub(crate) carga: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Interpretacion {
    pub(crate) estados: Vec<CambioDeEstado>,
    pub(crate) respuestas: Vec<RespuestaDeBoton>,
    /// Elementos que no se entendieron (texto libre, tipos desconocidos), solo para el registro.
    pub(crate) ignorados: usize,
}

#[derive(Debug, Deserialize)]
struct Carga {
    #[serde(default)]
    entry: Vec<Entrada>,
}

#[derive(Debug, Deserialize)]
struct Entrada {
    #[serde(default)]
    changes: Vec<Cambio>,
}

#[derive(Debug, Deserialize)]
struct Cambio {
    #[serde(default)]
    value: Valor,
}

#[derive(Debug, Default, Deserialize)]
struct Valor {
    #[serde(default)]
    statuses: Vec<EstadoMeta>,
    #[serde(default)]
    messages: Vec<MensajeEntrante>,
}

#[derive(Debug, Deserialize)]
struct EstadoMeta {
    id: String,
    status: String,
    #[serde(default)]
    errors: Vec<ErrorMeta>,
}

#[derive(Debug, Deserialize)]
struct ErrorMeta {
    code: Option<i64>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MensajeEntrante {
    #[serde(rename = "type", default)]
    tipo: String,
    context: Option<Contexto>,
    button: Option<Boton>,
    interactive: Option<Interactivo>,
}

#[derive(Debug, Deserialize)]
struct Contexto {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Boton {
    payload: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Interactivo {
    button_reply: Option<RespuestaInteractiva>,
}

#[derive(Debug, Deserialize)]
struct RespuestaInteractiva {
    id: String,
}

/// Interpreta la carga sin fallar: lo que no se entiende se cuenta como ignorado.
pub(crate) fn interpretar(carga: &Value) -> Interpretacion {
    let Ok(carga) = serde_json::from_value::<Carga>(carga.clone()) else {
        return Interpretacion {
            ignorados: 1,
            ..Default::default()
        };
    };
    let mut resultado = Interpretacion::default();
    for valor in carga.entry.into_iter().flat_map(|e| e.changes).map(|c| c.value) {
        for estado in valor.statuses {
            let estado_entrega = match estado.status.as_str() {
                "sent" => EstadoEntrega::Enviado,
                "delivered" => EstadoEntrega::Entregado,
                "read" => EstadoEntrega::Leido,
                "failed" => EstadoEntrega::Fallido,
                _ => {
                    resultado.ignorados += 1;
                    continue;
                }
            };
            let error = estado.errors.first().map(|e| match (e.code, &e.title) {
                (Some(codigo), Some(titulo)) => format!("{codigo}: {titulo}"),
                (Some(codigo), None) => codigo.to_string(),
                (None, Some(titulo)) => titulo.clone(),
                (None, None) => "error sin detalle".to_owned(),
            });
            resultado.estados.push(CambioDeEstado {
                id_proveedor: estado.id,
                estado: estado_entrega,
                error,
            });
        }
        for mensaje in valor.messages {
            let pulsado = match mensaje.tipo.as_str() {
                "button" => mensaje.button.and_then(|b| b.payload.or(b.text)),
                "interactive" => mensaje.interactive.and_then(|i| i.button_reply).map(|r| r.id),
                _ => None,
            };
            match (pulsado.as_deref().and_then(normalizar_carga), mensaje.context) {
                (Some(carga), Some(contexto)) => resultado.respuestas.push(RespuestaDeBoton {
                    id_original: contexto.id,
                    carga,
                }),
                _ => resultado.ignorados += 1,
            }
        }
    }
    resultado
}

/// Acepta la carga definida o, si Meta solo envía el texto del botón, el texto.
fn normalizar_carga(valor: &str) -> Option<&'static str> {
    let valor = valor.trim().to_lowercase();
    match valor.as_str() {
        "confirmar" | "confirmo" => Some(CONFIRMAR),
        "cancelar" | "cancelar cita" => Some(CANCELAR),
        _ => None,
    }
}

#[cfg(test)]
mod pruebas {
    use serde_json::json;

    use super::*;

    #[test]
    fn interpreta_estados_y_respuestas_de_botones() {
        let carga = json!({
            "object": "whatsapp_business_account",
            "entry": [{ "id": "WABA", "changes": [{ "field": "messages", "value": {
                "messaging_product": "whatsapp",
                "statuses": [
                    { "id": "wamid.A", "status": "delivered", "timestamp": "1763700000", "recipient_id": "573001112233" },
                    { "id": "wamid.B", "status": "failed", "errors": [{ "code": 131026, "title": "Message undeliverable" }] },
                    { "id": "wamid.C", "status": "deleted" }
                ],
                "messages": [
                    { "from": "573001112233", "id": "wamid.R1", "type": "button",
                      "context": { "from": "57300", "id": "wamid.A" },
                      "button": { "payload": "confirmar", "text": "Confirmo" } },
                    { "from": "573001112233", "id": "wamid.R2", "type": "button",
                      "context": { "id": "wamid.D" }, "button": { "text": "Cancelar cita" } },
                    { "from": "573001112233", "id": "wamid.R3", "type": "text", "text": { "body": "hola" } }
                ]
            }}]}]
        });
        let i = interpretar(&carga);
        assert_eq!(
            i.estados,
            [
                CambioDeEstado {
                    id_proveedor: "wamid.A".into(),
                    estado: EstadoEntrega::Entregado,
                    error: None
                },
                CambioDeEstado {
                    id_proveedor: "wamid.B".into(),
                    estado: EstadoEntrega::Fallido,
                    error: Some("131026: Message undeliverable".into())
                },
            ]
        );
        assert_eq!(
            i.respuestas,
            [
                RespuestaDeBoton {
                    id_original: "wamid.A".into(),
                    carga: CONFIRMAR
                },
                RespuestaDeBoton {
                    id_original: "wamid.D".into(),
                    carga: CANCELAR
                },
            ]
        );
        assert_eq!(i.ignorados, 2, "el estado desconocido y el texto libre");
        assert_eq!(interpretar(&json!("no es un objeto")).ignorados, 1);
    }

    #[test]
    fn los_estados_solo_avanzan() {
        assert!(puede_pasar("pendiente", EstadoEntrega::Enviado));
        assert!(puede_pasar("enviado", EstadoEntrega::Leido));
        assert!(!puede_pasar("leido", EstadoEntrega::Entregado), "desordenado");
        assert!(!puede_pasar("entregado", EstadoEntrega::Entregado));
        assert!(puede_pasar("entregado", EstadoEntrega::Fallido));
        assert!(!puede_pasar("leido", EstadoEntrega::Fallido));
        assert!(!puede_pasar("cancelado", EstadoEntrega::Enviado));
        assert!(!puede_pasar("fallido", EstadoEntrega::Entregado));
    }
}
