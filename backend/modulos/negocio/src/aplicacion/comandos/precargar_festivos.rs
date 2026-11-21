use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use chrono::Datelike;
use nucleo::{Actor, ErrorDeCampo};
use serde_json::json;

use crate::aplicacion::MODULO;
use crate::aplicacion::dto::{DiaEspecialDto, FestivosPrecargadosDto};
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;
use crate::dominio::entidades::DiaEspecial;
use crate::dominio::festivos::festivos_de_colombia;

/// Precarga los festivos de Colombia de un año (RF-CFG-05). Es idempotente: no toca las
/// fechas que ya tienen un día especial ni registra fechas pasadas.
#[derive(Debug)]
pub(crate) struct PrecargarFestivos<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> PrecargarFestivos<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        anio: i32,
    ) -> Result<FestivosPrecargadosDto, ErrorNegocio> {
        politicas::solo_administrador(actor)?;
        let hoy = self.p.reloj.hoy();
        if !(hoy.year()..=hoy.year() + 5).contains(&anio) {
            return Err(ErrorNegocio::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "anio",
                format!("Elige un año entre {} y {}.", hoy.year(), hoy.year() + 5),
            )]));
        }

        let mut cx = self.p.transacciones.iniciar().await?;
        let mut creados = Vec::new();
        let mut omitidos = 0;
        for (fecha, nombre) in festivos_de_colombia(anio) {
            if fecha < hoy {
                omitidos += 1;
                continue;
            }
            let dia = DiaEspecial::festivo_automatico(fecha, nombre);
            match self.p.dias.registrar_si_libre(&mut cx, &dia).await? {
                Some(id) => creados.push(DiaEspecialDto::desde(id, &dia)),
                None => omitidos += 1,
            }
        }
        self.p
            .auditor
            .registrar(
                &mut cx,
                EntradaAuditoria::nueva(actor, MODULO, "precargar_festivos", "dia_especial", anio)
                    .despues(&json!({ "creados": creados.len(), "omitidos": omitidos })),
            )
            .await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(FestivosPrecargadosDto {
            anio,
            creados,
            omitidos,
        })
    }
}
