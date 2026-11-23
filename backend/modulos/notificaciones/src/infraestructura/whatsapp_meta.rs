//! WhatsApp Cloud API (Meta): envía cada plantilla aprobada con sus parámetros en orden
//! (`POST {api}/{id_numero}/messages`, con el token de un usuario del sistema). Meta arma el
//! mensaje con el texto que tiene registrado; aquí solo viajan el nombre, el idioma y los
//! parámetros. El id que devuelve (`wamid…`) es el que después traen los webhooks de estado.

use std::time::Duration;

use async_trait::async_trait;
use plataforma::configuracion::{CloudApiWhatsApp, Secreto};
use serde_json::{Value, json};

use crate::aplicacion::puertos::{ErrorTransporte, PasarelaWhatsApp};
use crate::dominio::whatsapp::{WhatsAppRenderizado, definicion};

/// Tiempo máximo de un envío: el worker no debe quedarse esperando a Meta.
const TIEMPO_MAXIMO: Duration = Duration::from_secs(20);

pub(crate) struct WhatsAppCloudApi {
    cliente: reqwest::Client,
    url: String,
    token: Secreto,
    idioma: String,
}

impl std::fmt::Debug for WhatsAppCloudApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Sin el token.
        f.debug_struct("WhatsAppCloudApi")
            .field("url", &self.url)
            .field("idioma", &self.idioma)
            .finish_non_exhaustive()
    }
}

impl WhatsAppCloudApi {
    pub(crate) fn nuevo(c: &CloudApiWhatsApp) -> Result<Self, String> {
        let cliente = reqwest::Client::builder()
            .timeout(TIEMPO_MAXIMO)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cliente,
            url: format!("{}/{}/messages", c.api, c.id_numero),
            token: c.token.clone(),
            idioma: c.idioma.clone(),
        })
    }
}

/// `3001112233` → `573001112233`: el celular colombiano con el indicativo, sin «+».
pub(crate) fn numero_internacional(celular: &str) -> String {
    let digitos: String = celular.chars().filter(char::is_ascii_digit).collect();
    if digitos.len() == 10 {
        format!("57{digitos}")
    } else {
        digitos
    }
}

/// Cuerpo de la petición a Meta para una plantilla.
pub(crate) fn cuerpo(celular: &str, mensaje: &WhatsAppRenderizado, idioma: &str) -> Value {
    let texto = |t: &str| json!({ "type": "text", "text": t });
    let mut componentes = vec![json!({
        "type": "body",
        "parameters": mensaje.parametros.iter().map(|p| texto(p)).collect::<Vec<_>>(),
    })];
    let autenticacion = definicion(mensaje.plantilla).is_some_and(|d| d.categoria == "AUTHENTICATION");
    if autenticacion && let Some(codigo) = mensaje.parametros.first() {
        // El botón «Copiar código» de una plantilla de autenticación lleva el mismo código.
        componentes.push(json!({
            "type": "button", "sub_type": "url", "index": "0", "parameters": [texto(codigo)],
        }));
    }
    for (i, boton) in mensaje.botones.iter().enumerate() {
        // La carga vuelve en el webhook cuando el cliente toca el botón.
        componentes.push(json!({
            "type": "button", "sub_type": "quick_reply", "index": i.to_string(),
            "parameters": [{ "type": "payload", "payload": boton.carga }],
        }));
    }
    json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": numero_internacional(celular),
        "type": "template",
        "template": { "name": mensaje.plantilla, "language": { "code": idioma }, "components": componentes },
    })
}

/// Errores de Meta que se arreglan solos o con un cambio de configuración: se reintentan. Límites
/// de envío (4, 17, 32, 80007, 130429, 131048, 131056), fallas de Meta (1, 2, 131000, 131016,
/// 133004), bloqueo temporal (368) y credenciales (190 token vencido o inválido; 10 y 200
/// permisos), que alguien corrige en Dokploy mientras el mensaje espera.
const TEMPORALES: [i64; 15] = [
    1, 2, 4, 10, 17, 32, 190, 200, 368, 80_007, 130_429, 131_000, 131_016, 131_048, 133_004,
];

/// Traduce una respuesta de error de Meta. Los demás `4xx` son permanentes: el número no tiene
/// WhatsApp (131026), la plantilla no existe o no coincide (132xxx), un parámetro no sirve…
pub(crate) fn clasificar(estado: u16, respuesta: &Value) -> ErrorTransporte {
    let error = &respuesta["error"];
    let codigo = error["code"].as_i64();
    let detalle = error["error_data"]["details"]
        .as_str()
        .or_else(|| error["message"].as_str())
        .unwrap_or("sin detalle");
    let prefijo = codigo.map_or_else(|| format!("HTTP {estado}"), |c| c.to_string());
    let mensaje: String = format!("Meta {prefijo}: {detalle}").chars().take(300).collect();
    let temporal =
        estado == 429 || estado >= 500 || codigo.is_some_and(|c| TEMPORALES.contains(&c) || c == 131_056);
    ErrorTransporte {
        mensaje,
        permanente: !temporal,
    }
}

#[async_trait]
impl PasarelaWhatsApp for WhatsAppCloudApi {
    async fn enviar(&self, celular: &str, mensaje: &WhatsAppRenderizado) -> Result<String, ErrorTransporte> {
        let respuesta = self
            .cliente
            .post(&self.url)
            .bearer_auth(self.token.exponer())
            .json(&cuerpo(celular, mensaje, &self.idioma))
            .send()
            .await
            .map_err(|e| ErrorTransporte {
                // Sin la URL: no lleva secretos, pero tampoco aporta.
                mensaje: format!("sin conexión con Meta: {}", e.without_url()),
                permanente: false,
            })?;
        let estado = respuesta.status().as_u16();
        let cuerpo: Value = respuesta.json().await.unwrap_or(Value::Null);
        if !(200..300).contains(&estado) {
            let error = clasificar(estado, &cuerpo);
            if !error.permanente && matches!(cuerpo["error"]["code"].as_i64(), Some(10 | 190 | 200)) {
                tracing::error!(error = error.mensaje, "Meta rechazó las credenciales de WhatsApp");
            }
            return Err(error);
        }
        cuerpo["messages"][0]["id"]
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| ErrorTransporte {
                mensaje: "Meta respondió sin el id del mensaje".into(),
                permanente: false,
            })
    }
}

#[cfg(test)]
mod pruebas {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;

    use super::*;
    use crate::api::PlantillaWhatsApp;
    use crate::dominio::whatsapp::renderizar;

    fn recordatorio() -> WhatsAppRenderizado {
        renderizar(
            &PlantillaWhatsApp::RecordatorioCita {
                nombre: "Andrea".into(),
                servicio: "Corte clásico".into(),
                barbero: "Daniel".into(),
                fecha: "lunes 23 de noviembre".into(),
                hora: "9:00 a. m.".into(),
            },
            "Diamante Barbería",
        )
    }

    #[test]
    fn el_numero_lleva_el_indicativo_de_colombia() {
        assert_eq!(numero_internacional("3001112233"), "573001112233");
        assert_eq!(numero_internacional("300 111 2233"), "573001112233");
        assert_eq!(numero_internacional("573001112233"), "573001112233");
    }

    #[test]
    fn el_recordatorio_lleva_parametros_idioma_y_cargas_de_los_botones() {
        assert_eq!(
            cuerpo("3001112233", &recordatorio(), "es_CO"),
            json!({
                "messaging_product": "whatsapp", "recipient_type": "individual", "to": "573001112233",
                "type": "template",
                "template": { "name": "recordatorio_cita", "language": { "code": "es_CO" }, "components": [
                    { "type": "body", "parameters": [
                        { "type": "text", "text": "Andrea" }, { "type": "text", "text": "Diamante Barbería" },
                        { "type": "text", "text": "lunes 23 de noviembre" }, { "type": "text", "text": "9:00 a. m." },
                        { "type": "text", "text": "Daniel" } ] },
                    { "type": "button", "sub_type": "quick_reply", "index": "0",
                      "parameters": [{ "type": "payload", "payload": "confirmar" }] },
                    { "type": "button", "sub_type": "quick_reply", "index": "1",
                      "parameters": [{ "type": "payload", "payload": "cancelar" }] } ] }
            })
        );
    }

    #[test]
    fn el_codigo_va_en_el_cuerpo_y_en_el_boton_copiar_codigo() {
        let codigo = renderizar(
            &PlantillaWhatsApp::CodigoVerificacion {
                codigo: "4821".into(),
            },
            "Diamante Barbería",
        );
        let c = cuerpo("3001112233", &codigo, "es_CO");
        assert_eq!(c["template"]["name"], "codigo_verificacion");
        assert_eq!(
            c["template"]["components"],
            json!([
                { "type": "body", "parameters": [{ "type": "text", "text": "4821" }] },
                { "type": "button", "sub_type": "url", "index": "0", "parameters": [{ "type": "text", "text": "4821" }] }
            ])
        );
    }

    #[test]
    fn clasifica_los_errores_de_meta() {
        let error = |codigo: i64| json!({ "error": { "message": "x", "code": codigo, "error_data": { "details": "detalle" } } });
        for (estado, codigo, permanente) in [
            (400, 131_026, true),  // el número no tiene WhatsApp
            (400, 132_001, true),  // la plantilla no existe en ese idioma
            (400, 132_000, true),  // no coincide el número de parámetros
            (400, 131_030, true),  // número de prueba fuera de la lista permitida
            (400, 130_429, false), // límite de envío
            (400, 131_056, false), // demasiados mensajes al mismo número
            (401, 190, false),     // token vencido: se corrige y se reintenta
            (500, 131_000, false), // falla de Meta
        ] {
            let e = clasificar(estado, &error(codigo));
            assert_eq!(e.permanente, permanente, "{codigo}");
            assert_eq!(e.mensaje, format!("Meta {codigo}: detalle"));
        }
        let e = clasificar(502, &Value::Null);
        assert!(!e.permanente && e.mensaje.starts_with("Meta HTTP 502"));
        assert!(clasificar(429, &Value::Null).mensaje.contains("429"));
    }

    /// Meta de mentira: anota cada petición y responde según el celular de destino.
    #[derive(Clone, Default)]
    struct Meta(Arc<Mutex<Vec<(String, String, Value)>>>);

    async fn mensajes(
        State(meta): State<Meta>,
        cabeceras: HeaderMap,
        cuerpo: String,
    ) -> (StatusCode, String) {
        let json: Value = serde_json::from_str(&cuerpo).unwrap();
        let autorizacion = cabeceras["authorization"].to_str().unwrap().to_owned();
        let destino = json["to"].as_str().unwrap().to_owned();
        meta.0.lock().unwrap().push((autorizacion, destino.clone(), json));
        match destino.as_str() {
            "573009999999" => (
                StatusCode::BAD_REQUEST,
                json!({ "error": { "code": 131_026, "message": "Message undeliverable" } }).to_string(),
            ),
            "573000000000" => (StatusCode::SERVICE_UNAVAILABLE, "no disponible".into()),
            _ => (
                StatusCode::OK,
                json!({ "messaging_product": "whatsapp", "contacts": [{ "input": destino, "wa_id": destino }],
                        "messages": [{ "id": "wamid.HBgMNTczMDAxMTEyMjMzFQIAERgS" }] })
                .to_string(),
            ),
        }
    }

    #[tokio::test]
    async fn envia_a_la_api_de_meta_y_devuelve_el_wamid() {
        let meta = Meta::default();
        let app = Router::new()
            .route("/v26.0/{numero}/messages", post(mensajes))
            .with_state(meta.clone());
        let escucha = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let direccion = escucha.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(escucha, app).await.unwrap() });
        let pasarela = WhatsAppCloudApi::nuevo(&CloudApiWhatsApp {
            token: Secreto::nuevo("token-de-prueba"),
            id_numero: "1234567890".into(),
            idioma: "es_CO".into(),
            api: format!("http://{direccion}/v26.0"),
        })
        .unwrap();
        assert!(!format!("{pasarela:?}").contains("token-de-prueba"));

        let id = pasarela.enviar("3001112233", &recordatorio()).await.unwrap();
        assert_eq!(id, "wamid.HBgMNTczMDAxMTEyMjMzFQIAERgS");
        let error = pasarela.enviar("3009999999", &recordatorio()).await.unwrap_err();
        assert!(
            error.permanente && error.mensaje.starts_with("Meta 131026"),
            "{error:?}"
        );
        let error = pasarela.enviar("3000000000", &recordatorio()).await.unwrap_err();
        assert!(!error.permanente, "{error:?}");

        let recibidas = meta.0.lock().unwrap().clone();
        assert_eq!(recibidas.len(), 3);
        let (autorizacion, destino, json) = &recibidas[0];
        assert_eq!(autorizacion, "Bearer token-de-prueba");
        assert_eq!(destino, "573001112233");
        assert_eq!(json["template"]["name"], "recordatorio_cita");
        assert_eq!(json["template"]["language"]["code"], "es_CO");

        // Sin Meta (nadie escucha): falla temporal, se reintenta.
        let caida = WhatsAppCloudApi::nuevo(&CloudApiWhatsApp {
            token: Secreto::nuevo("t"),
            id_numero: "1".into(),
            idioma: "es_CO".into(),
            api: "http://127.0.0.1:1/v26.0".into(),
        })
        .unwrap();
        let error = caida.enviar("3001112233", &recordatorio()).await.unwrap_err();
        assert!(
            !error.permanente && error.mensaje.starts_with("sin conexión con Meta"),
            "{error:?}"
        );
    }
}
