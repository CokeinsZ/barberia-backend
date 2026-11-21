//! WhatsApp simulado (desarrollo y pruebas): no llama a Meta; registra en el log el texto exacto
//! de la plantilla y devuelve un id `wamid.simulado.…`. La configuración impide usarlo en
//! producción. Los mensajes siguen en `notificaciones.mensaje` con sus variables, y las pruebas
//! los leen con `GET /api/v1/_pruebas/mensajes`.
//!
//! Para probar fallas sin depender de nada externo:
//!
//! - celular terminado en `0000`: falla temporal (se reintenta);
//! - celular terminado en `9999`: falla permanente (número sin WhatsApp).

use async_trait::async_trait;
use plataforma::seguridad::generar_token;

use crate::aplicacion::puertos::{ErrorTransporte, PasarelaWhatsApp};
use crate::dominio::whatsapp::WhatsAppRenderizado;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct WhatsAppSimulado;

#[async_trait]
impl PasarelaWhatsApp for WhatsAppSimulado {
    async fn enviar(&self, celular: &str, mensaje: &WhatsAppRenderizado) -> Result<String, ErrorTransporte> {
        if celular.ends_with("0000") {
            return Err(ErrorTransporte {
                mensaje: "WhatsApp simulado: sin conexión con el proveedor".into(),
                permanente: false,
            });
        }
        if celular.ends_with("9999") {
            return Err(ErrorTransporte {
                mensaje: "131026: el número no tiene WhatsApp (simulado)".into(),
                permanente: true,
            });
        }
        let (aleatorio, _) = generar_token().map_err(|e| ErrorTransporte {
            mensaje: e.to_string(),
            permanente: false,
        })?;
        let botones: Vec<&str> = mensaje.botones.iter().map(|b| b.texto).collect();
        tracing::info!(
            celular,
            plantilla = mensaje.plantilla,
            botones = ?botones,
            texto = mensaje.texto,
            "WhatsApp simulado"
        );
        Ok(format!("wamid.simulado.{aleatorio}"))
    }

    fn conserva_variables(&self) -> bool {
        true
    }
}
