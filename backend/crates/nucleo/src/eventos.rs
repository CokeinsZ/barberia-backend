//! Eventos de dominio y bus en proceso (patrones Observer y Mediator).
//!
//! Los módulos se comunican con eventos cuando una dependencia directa crearía un ciclo
//! (p. ej. `ventas` publica `VentaRegistrada` y `agenda` marca la cita como atendida).
//! El bus es **síncrono**: cada manejador recibe el mismo contexto (normalmente la unidad de
//! trabajo con la transacción abierta), así los cambios de todos quedan en una sola transacción.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

/// Hecho ocurrido en un módulo que otros pueden necesitar conocer.
pub trait EventoDominio: Send + Sync + 'static {
    /// Nombre estable, p. ej. `ventas.venta_registrada`.
    fn nombre(&self) -> &'static str;
}

/// Reacción de un módulo a un evento. `C` es el contexto compartido (la unidad de trabajo).
#[async_trait]
pub trait ManejadorEvento<E: EventoDominio, C: Send>: Send + Sync + 'static {
    /// Nombre del manejador, para los mensajes de error.
    fn nombre(&self) -> &'static str;

    async fn manejar(&self, contexto: &mut C, evento: &E) -> Result<(), ErrorDeManejador>;
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ErrorDeManejador(pub String);

impl ErrorDeManejador {
    pub fn nuevo(causa: impl fmt::Display) -> Self {
        Self(causa.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("El manejador «{manejador}» falló al procesar «{evento}»: {causa}")]
pub struct ErrorDePublicacion {
    pub evento: &'static str,
    pub manejador: &'static str,
    pub causa: ErrorDeManejador,
}

#[async_trait]
trait ManejadorBorrado<C: Send>: Send + Sync {
    fn nombre(&self) -> &'static str;
    async fn manejar(
        &self,
        contexto: &mut C,
        evento: &(dyn Any + Send + Sync),
    ) -> Result<(), ErrorDeManejador>;
}

struct Adaptador<E, H> {
    manejador: H,
    _evento: PhantomData<fn(E)>,
}

#[async_trait]
impl<E, C, H> ManejadorBorrado<C> for Adaptador<E, H>
where
    E: EventoDominio,
    C: Send,
    H: ManejadorEvento<E, C>,
{
    fn nombre(&self) -> &'static str {
        self.manejador.nombre()
    }

    async fn manejar(
        &self,
        contexto: &mut C,
        evento: &(dyn Any + Send + Sync),
    ) -> Result<(), ErrorDeManejador> {
        match evento.downcast_ref::<E>() {
            Some(evento) => self.manejador.manejar(contexto, evento).await,
            None => Ok(()),
        }
    }
}

/// Bus de eventos. Las suscripciones se registran en la raíz de composición (`app`).
/// Manejadores de cada tipo de evento.
type Suscripciones<C> = HashMap<TypeId, Vec<Arc<dyn ManejadorBorrado<C>>>>;

pub struct BusEventos<C> {
    // Las suscripciones se agregan al armar la aplicación, incluso después de repartir el bus
    // (los manejadores de un módulo pueden necesitar fachadas que se construyen más tarde).
    manejadores: RwLock<Suscripciones<C>>,
}

impl<C> fmt::Debug for BusEventos<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BusEventos")
            .field("tipos_de_evento", &self.manejadores.read().map_or(0, |m| m.len()))
            .finish()
    }
}

impl<C: Send + 'static> Default for BusEventos<C> {
    fn default() -> Self {
        Self {
            manejadores: RwLock::new(HashMap::new()),
        }
    }
}

impl<C: Send + 'static> BusEventos<C> {
    pub fn nuevo() -> Self {
        Self::default()
    }

    fn leer(&self) -> std::sync::RwLockReadGuard<'_, Suscripciones<C>> {
        self.manejadores.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Registra un manejador. Los manejadores de un mismo evento se ejecutan en orden de registro.
    pub fn suscribir<E, H>(&self, manejador: H)
    where
        E: EventoDominio,
        H: ManejadorEvento<E, C>,
    {
        let adaptador = Adaptador {
            manejador,
            _evento: PhantomData::<fn(E)>,
        };
        self.manejadores
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Arc::new(adaptador));
    }

    /// Entrega el evento a todos sus manejadores. Se detiene en el primer error, para que la
    /// transacción completa se revierta.
    pub async fn publicar<E: EventoDominio>(
        &self,
        contexto: &mut C,
        evento: &E,
    ) -> Result<(), ErrorDePublicacion> {
        // Copia de la lista: el candado no se mantiene durante los `await`, así un manejador
        // puede publicar otros eventos.
        let Some(manejadores) = self.leer().get(&TypeId::of::<E>()).cloned() else {
            return Ok(());
        };
        for manejador in &manejadores {
            manejador
                .manejar(contexto, evento)
                .await
                .map_err(|causa| ErrorDePublicacion {
                    evento: evento.nombre(),
                    manejador: manejador.nombre(),
                    causa,
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    struct VentaRegistrada(i64);
    impl EventoDominio for VentaRegistrada {
        fn nombre(&self) -> &'static str {
            "ventas.venta_registrada"
        }
    }

    struct Otro;
    impl EventoDominio for Otro {
        fn nombre(&self) -> &'static str {
            "otro"
        }
    }

    struct Anotar(&'static str);
    #[async_trait]
    impl ManejadorEvento<VentaRegistrada, Vec<String>> for Anotar {
        fn nombre(&self) -> &'static str {
            self.0
        }
        async fn manejar(
            &self,
            contexto: &mut Vec<String>,
            evento: &VentaRegistrada,
        ) -> Result<(), ErrorDeManejador> {
            contexto.push(format!("{}:{}", self.0, evento.0));
            Ok(())
        }
    }

    struct Fallar;
    #[async_trait]
    impl ManejadorEvento<VentaRegistrada, Vec<String>> for Fallar {
        fn nombre(&self) -> &'static str {
            "fallar"
        }
        async fn manejar(&self, _: &mut Vec<String>, _: &VentaRegistrada) -> Result<(), ErrorDeManejador> {
            Err(ErrorDeManejador::nuevo("sin conexión"))
        }
    }

    #[tokio::test]
    async fn entrega_en_orden_y_solo_a_los_suscritos() {
        let bus = BusEventos::<Vec<String>>::nuevo();
        bus.suscribir(Anotar("agenda"));
        bus.suscribir(Anotar("clientes"));
        let mut contexto = Vec::new();
        bus.publicar(&mut contexto, &VentaRegistrada(7)).await.unwrap();
        bus.publicar(&mut contexto, &Otro).await.unwrap();
        assert_eq!(contexto, ["agenda:7", "clientes:7"]);
    }

    #[tokio::test]
    async fn se_detiene_en_el_primer_error() {
        let bus = BusEventos::<Vec<String>>::nuevo();
        bus.suscribir(Fallar);
        bus.suscribir(Anotar("clientes"));
        let mut contexto = Vec::new();
        let error = bus
            .publicar(&mut contexto, &VentaRegistrada(1))
            .await
            .unwrap_err();
        assert_eq!(error.manejador, "fallar");
        assert_eq!(error.evento, "ventas.venta_registrada");
        assert!(contexto.is_empty());
    }
}
