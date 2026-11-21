use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use notificaciones::api::{PlantillaWhatsApp, WhatsAppSaliente};
use nucleo::{Celular, ErrorDeCampo};
use plataforma::seguridad::{generar_token, huella_token};

use super::puertos::Puertos;
use crate::api::{CodigoEnviadoDto, Comprobantes, ErrorVerificacion, VerificacionDto};
use crate::dominio::{
    ESPERA_REENVIO, VIGENCIA_CODIGO, VIGENCIA_COMPROBANTE, generar_codigo, iguales, mensaje_codigo,
};

/// Casos de uso del módulo, sobre los mismos puertos.
pub(crate) struct Verificacion<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Verificacion<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Verificacion")
    }
}

impl<C: Send> Verificacion<C> {
    /// Envía un código nuevo por WhatsApp (RF-RES-09). Responde lo mismo exista o no el cliente.
    pub(crate) async fn solicitar(
        &self,
        celular: &str,
        ip: Option<IpAddr>,
    ) -> Result<CodigoEnviadoDto, ErrorVerificacion> {
        let celular = Celular::nuevo(celular).map_err(|e| {
            ErrorVerificacion::DatosInvalidos(vec![ErrorDeCampo::nuevo("celular", e.to_string())])
        })?;
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        if let Some(ultimo) = p.codigos.ultimo(&mut cx, &celular).await?
            && ultimo + ESPERA_REENVIO > ahora
        {
            let segundos = (ultimo + ESPERA_REENVIO - ahora).num_seconds().max(1);
            return Err(ErrorVerificacion::EsperaReenvio {
                segundos: u64::try_from(segundos).unwrap_or(60),
            });
        }
        let codigo = generar_codigo().map_err(ErrorVerificacion::Interno)?;
        let huella = p.firmador.huella(&mensaje_codigo(celular.digitos(), &codigo));
        let expira_en = ahora + VIGENCIA_CODIGO;
        let id = p
            .codigos
            .crear(&mut cx, &celular, &huella, expira_en, ip, ahora)
            .await?;
        p.buzon
            .encolar_whatsapp(
                &mut cx,
                WhatsAppSaliente {
                    destinatario: celular,
                    plantilla: PlantillaWhatsApp::CodigoVerificacion { codigo },
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(CodigoEnviadoDto {
            id,
            expira_en,
            reenviar_desde: ahora + ESPERA_REENVIO,
        })
    }

    /// Confirma el código. Cada fallo cuenta (y se guarda) aunque la respuesta sea un error; al
    /// agotar los intentos el código queda invalidado. Al acertar, entrega el comprobante y los
    /// datos del cliente conocido (solo ahora, RF-RES-08).
    pub(crate) async fn confirmar(
        &self,
        id: i64,
        codigo: &str,
    ) -> Result<VerificacionDto, ErrorVerificacion> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let guardado = p
            .codigos
            .para_confirmar(&mut cx, id)
            .await?
            .ok_or(ErrorVerificacion::NoEncontrado)?;
        if guardado.verificado || guardado.expira_en <= ahora || guardado.intentos >= guardado.max_intentos {
            return Err(ErrorVerificacion::CodigoVencido);
        }
        let codigo = codigo.trim();
        let huella = p.firmador.huella(&mensaje_codigo(&guardado.celular, codigo));
        if !iguales(&huella, &guardado.huella) {
            let intentos = p.codigos.registrar_fallo(&mut cx, id).await?;
            p.transacciones.confirmar(cx).await?;
            let restantes = guardado.max_intentos - intentos;
            return Err(if restantes <= 0 {
                ErrorVerificacion::CodigoVencido
            } else {
                ErrorVerificacion::CodigoIncorrecto { restantes }
            });
        }
        let (comprobante, huella_comprobante) =
            generar_token().map_err(|e| ErrorVerificacion::Interno(e.0))?;
        p.codigos
            .marcar_verificado(
                &mut cx,
                id,
                &huella_comprobante,
                ahora + VIGENCIA_COMPROBANTE,
                ahora,
            )
            .await?;
        let celular =
            Celular::nuevo(&guardado.celular).map_err(|e| ErrorVerificacion::Interno(e.to_string()))?;
        let cliente = p.clientes.datos_de(&mut cx, &celular).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(VerificacionDto { comprobante, cliente })
    }
}

#[async_trait]
impl<C: Send> Comprobantes<C> for Verificacion<C> {
    async fn consumir(&self, cx: &mut C, comprobante: &str) -> Result<Celular, ErrorVerificacion> {
        let celular = self
            .p
            .codigos
            .consumir_comprobante(cx, &huella_token(comprobante), self.p.reloj.ahora())
            .await?
            .ok_or(ErrorVerificacion::ComprobanteVencido)?;
        Celular::nuevo(&celular).map_err(|e| ErrorVerificacion::Interno(e.to_string()))
    }
}
