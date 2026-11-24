//! WhatsApp de verdad solo a unos celulares (`DIAMANTE_WHATSAPP_SOLO_A`), para preproducción.
//!
//! Preproducción envía por Meta, pero tiene clientes con celulares inventados (los datos de
//! prueba y los que crea el equipo mientras desarrolla). Con la lista, los mensajes a esos
//! celulares salen por Meta y a los demás se les simula el envío, como en desarrollo: quedan
//! «enviados» con un id `wamid.simulado.…`, conservan sus variables y nunca salen del sistema.
//! En producción no se define la variable y todo sale por Meta.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::aplicacion::puertos::{ErrorTransporte, PasarelaWhatsApp};
use crate::dominio::whatsapp::WhatsAppRenderizado;
use crate::infraestructura::whatsapp_simulado::WhatsAppSimulado;

#[derive(Debug)]
pub(crate) struct WhatsAppRestringido {
    real: Arc<dyn PasarelaWhatsApp>,
    simulado: WhatsAppSimulado,
    permitidos: HashSet<String>,
}

impl WhatsAppRestringido {
    pub(crate) fn nuevo(real: Arc<dyn PasarelaWhatsApp>, permitidos: &[String]) -> Self {
        Self {
            real,
            simulado: WhatsAppSimulado,
            permitidos: permitidos.iter().cloned().collect(),
        }
    }

    fn permitido(&self, celular: &str) -> bool {
        self.permitidos.contains(celular)
    }
}

#[async_trait]
impl PasarelaWhatsApp for WhatsAppRestringido {
    async fn enviar(&self, celular: &str, mensaje: &WhatsAppRenderizado) -> Result<String, ErrorTransporte> {
        if self.permitido(celular) {
            self.real.enviar(celular, mensaje).await
        } else {
            tracing::info!(
                plantilla = mensaje.plantilla,
                "WhatsApp simulado: el celular no está en DIAMANTE_WHATSAPP_SOLO_A"
            );
            self.simulado.enviar(celular, mensaje).await
        }
    }

    fn conserva_variables(&self, celular: &str) -> bool {
        if self.permitido(celular) {
            self.real.conserva_variables(celular)
        } else {
            self.simulado.conserva_variables(celular)
        }
    }
}

#[cfg(test)]
mod pruebas {
    use std::sync::Mutex;

    use super::*;
    use crate::api::PlantillaWhatsApp;
    use crate::dominio::whatsapp::renderizar;

    /// Meta de mentira: anota a quién se le envió.
    #[derive(Debug, Default)]
    struct MetaDePrueba {
        enviados: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl PasarelaWhatsApp for MetaDePrueba {
        async fn enviar(&self, celular: &str, _: &WhatsAppRenderizado) -> Result<String, ErrorTransporte> {
            self.enviados.lock().unwrap().push(celular.to_owned());
            Ok("wamid.meta.1".into())
        }
    }

    #[tokio::test]
    async fn solo_los_celulares_de_la_lista_salen_por_meta() {
        let meta = Arc::new(MetaDePrueba::default());
        let pasarela = WhatsAppRestringido::nuevo(meta.clone(), &["3003579384".to_owned()]);
        let mensaje = renderizar(
            &PlantillaWhatsApp::CodigoVerificacion {
                codigo: "4821".into(),
            },
            "Diamante Barbería",
        );

        assert_eq!(
            pasarela.enviar("3003579384", &mensaje).await.unwrap(),
            "wamid.meta.1"
        );
        let simulado = pasarela.enviar("3209990001", &mensaje).await.unwrap();
        assert!(simulado.starts_with("wamid.simulado."), "{simulado}");
        assert_eq!(
            *meta.enviados.lock().unwrap(),
            ["3003579384"],
            "a Meta solo va el de la lista"
        );

        // Los simulados conservan sus variables (nunca salen); los reales, como Meta.
        assert!(!pasarela.conserva_variables("3003579384"));
        assert!(pasarela.conserva_variables("3209990001"));
    }
}
