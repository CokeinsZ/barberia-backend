use std::collections::HashSet;

use chrono::NaiveDate;
use contratos::barberos::{AccesoRevocado, BarberoRetirado};
use nucleo::identificadores::IdBarbero;
use nucleo::{Actor, ErrorDeCampo};
use serde_json::json;

use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::BarberoDto;
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::solo_administrador;
use crate::dominio::{EstadoBarbero, Retiro};

/// Interruptores de la ficha (RF-BAR-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SolicitudVisibilidad {
    pub(crate) mostrar_en_landing: bool,
    pub(crate) recibe_citas_en_linea: bool,
    pub(crate) acceso_panel: bool,
}

impl Default for SolicitudVisibilidad {
    fn default() -> Self {
        Self {
            mostrar_en_landing: true,
            recibe_citas_en_linea: true,
            acceso_panel: true,
        }
    }
}

caso_de_uso!(
    /// «Mostrar en la landing», «Recibe citas en línea» y «Acceso al panel» (RF-BAR-09,
    /// RF-AUT-08). Dar acceso envía la invitación si la cuenta no se ha activado; quitarlo
    /// cierra sus sesiones.
    CambiarVisibilidad
);

impl<C: Send + 'static> CambiarVisibilidad<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        s: SolicitudVisibilidad,
    ) -> Result<BarberoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let antes = p.ficha(&mut cx, id).await?;
        let enciende_algo = s.mostrar_en_landing || s.recibe_citas_en_linea || s.acceso_panel;
        if antes.estado.codigo == EstadoBarbero::Inactivo.codigo() && enciende_algo {
            return Err(ErrorBarberos::BarberoRetirado);
        }
        p.barberos
            .cambiar_visibilidad(&mut cx, id, s.mostrar_en_landing, s.recibe_citas_en_linea)
            .await?;
        if antes.visibilidad.acceso_panel != s.acceso_panel {
            p.cuentas
                .cambiar_acceso(&mut cx, actor, antes.usuario_id, s.acceso_panel, &antes.nombre)
                .await?;
            if !s.acceso_panel {
                p.publicar(
                    &mut cx,
                    &AccesoRevocado {
                        barbero: id,
                        usuario: antes.usuario_id,
                    },
                )
                .await?;
            }
        }
        let despues = p.ficha(&mut cx, id).await?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "cambiar_visibilidad", id)
                .antes(&antes.visibilidad)
                .despues(&despues.visibilidad),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}

caso_de_uso!(
    /// Orden del equipo (RF-BAR-19): el de la landing y el de «Cualquier barbero» (RN-06, DA-03).
    /// La lista debe traer a todos los barberos vigentes, una vez cada uno.
    OrdenarEquipo
);

impl<C: Send + 'static> OrdenarEquipo<C> {
    pub(crate) async fn ejecutar(&self, actor: &Actor, ids: Vec<IdBarbero>) -> Result<(), ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let vigentes = p.barberos.vigentes(&mut cx).await?;
        let pedidos: HashSet<IdBarbero> = ids.iter().copied().collect();
        if pedidos.len() != ids.len() || pedidos != vigentes.iter().copied().collect::<HashSet<_>>() {
            return Err(ErrorBarberos::DatosInvalidos(vec![ErrorDeCampo::nuevo(
                "barbero_ids",
                "Incluye a todos los barberos vigentes, una vez cada uno.",
            )]));
        }
        p.barberos.reordenar(&mut cx, &ids).await?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "ordenar_equipo", "equipo")
                .antes(&vigentes)
                .despues(&ids),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(())
    }
}

caso_de_uso!(
    /// Retira a un barbero (RF-BAR-11, RF-BAR-15, RN-25) con fecha y motivo. Desde esa fecha
    /// deja de aparecer y de recibir citas; si la fecha ya llegó, pierde el acceso al panel y se
    /// cierran sus sesiones. Su historial se conserva.
    RetirarBarbero
);

impl<C: Send + 'static> RetirarBarbero<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        fecha: NaiveDate,
        motivo: &str,
    ) -> Result<BarberoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let barbero = p.guardado(&mut cx, id).await?;
        if barbero.tiene_retiro() {
            return Err(ErrorBarberos::BarberoRetirado);
        }
        let retiro = Retiro::nuevo(fecha, motivo, barbero.fecha_ingreso, p.hoy())?;
        p.barberos.retirar(&mut cx, id, &retiro).await?;
        p.publicar(
            &mut cx,
            &BarberoRetirado {
                barbero: id,
                fecha: retiro.fecha,
            },
        )
        .await?;
        if retiro.fecha <= p.hoy() {
            p.cuentas
                .cambiar_acceso(&mut cx, actor, barbero.usuario, false, &barbero.nombre())
                .await?;
            p.publicar(
                &mut cx,
                &AccesoRevocado {
                    barbero: id,
                    usuario: barbero.usuario,
                },
            )
            .await?;
        }
        p.auditar(
            &mut cx,
            p.entrada(actor, "retirar_barbero", id)
                .despues(&json!({ "fecha_retiro": retiro.fecha, "motivo": retiro.motivo.codigo() })),
        )
        .await?;
        let ficha = p.ficha(&mut cx, id).await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ficha)
    }
}
