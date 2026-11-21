//! Recordatorios automáticos por WhatsApp (RF-NOT-02): los envía la tarea periódica del worker.

use std::sync::Arc;

use chrono::TimeDelta;
use nucleo::ErrorPersistencia;

use super::errores::ErrorAgenda;
use super::puertos::Puertos;
use crate::api::ResumenRecordatorios;
use crate::dominio::recordatorios::{ReglaRecordatorios, recordatorio_debido};

pub(crate) struct Recordatorios<C: Send> {
    pub(crate) p: Arc<Puertos<C>>,
}

impl<C: Send> std::fmt::Debug for Recordatorios<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Recordatorios")
    }
}

impl<C: Send + 'static> Recordatorios<C> {
    /// Encola los recordatorios que correspondan ahora. Es idempotente: ejecutarla dos veces no
    /// repite ninguno (E2E-NOT-01), y si dos workers compiten, el índice único de
    /// `agenda.mensaje_cita` deja pasar a uno solo.
    pub(crate) async fn enviar_debidos(&self) -> Result<ResumenRecordatorios, ErrorAgenda> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let mut cx = p.transacciones.iniciar().await?;
        let configuracion = p.citas.configuracion(&mut cx).await?;
        let regla = ReglaRecordatorios {
            con_24h: configuracion.recordatorio_24h,
            con_2h: configuracion.recordatorio_2h,
        };
        let candidatas = p
            .citas
            .citas_para_recordar(&mut cx, ahora, ahora + TimeDelta::hours(24))
            .await?;
        let mut ids: Vec<_> = candidatas.iter().map(|c| c.cita.cliente).collect();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        let clientes = p.clientes.por_ids(&mut cx, &ids).await?;
        p.transacciones.confirmar(cx).await?;

        let mut resumen = ResumenRecordatorios::default();
        for candidata in candidatas {
            let Some(recordatorio) = recordatorio_debido(
                regla,
                candidata.cita.inicio,
                candidata.vigente_desde,
                ahora,
                &candidata.recordatorios_enviados,
            ) else {
                continue;
            };
            // RF-RES-18: el cliente puede desactivarlos; un cliente anonimizado no tiene celular.
            let activo = clientes
                .iter()
                .find(|c| c.id == candidata.cita.cliente)
                .is_some_and(|c| c.recordatorios_activos);
            if !activo {
                resumen.omitidos += 1;
                continue;
            }
            let mut cx = p.transacciones.iniciar().await?;
            match p
                .avisar(&mut cx, &candidata.cita, recordatorio.proposito(), None)
                .await
            {
                Ok(()) => {
                    p.transacciones.confirmar(cx).await?;
                    resumen.enviados += 1;
                }
                // Otro proceso lo envió entre medio: se descarta este intento.
                Err(ErrorAgenda::Persistencia(ErrorPersistencia::Duplicado { .. })) => resumen.omitidos += 1,
                Err(otro) => return Err(otro),
            }
        }
        Ok(resumen)
    }
}
