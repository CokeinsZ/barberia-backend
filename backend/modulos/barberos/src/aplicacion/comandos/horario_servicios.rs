use contratos::barberos::HorarioCambiado;
use nucleo::Actor;
use nucleo::identificadores::{IdBarbero, IdServicio};

use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::{HorarioDto, ServicioOfrecidoDto};
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::administrador_o_propio;
use crate::dominio::{EstadoBarbero, Franja, Horario};

caso_de_uso!(
    /// Horario semanal (RF-BAR-07, RF-PBA-09). El barbero puede cambiar el suyo sin aprobación
    /// (DA-08): queda auditado y se publica `HorarioCambiado` para avisar de citas afectadas.
    GestionHorario
);

impl<C: Send + 'static> GestionHorario<C> {
    pub(crate) async fn consultar(&self, actor: &Actor, id: IdBarbero) -> Result<HorarioDto, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let horario = Horario::reconstruir(p.barberos.franjas(&mut cx, id).await?);
        p.transacciones.confirmar(cx).await?;
        Ok(HorarioDto::from(&horario))
    }

    pub(crate) async fn cambiar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        franjas: Vec<Franja>,
    ) -> Result<HorarioDto, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let horario = Horario::nuevo(franjas)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        if p.guardado(&mut cx, id).await?.estado == EstadoBarbero::Inactivo {
            return Err(ErrorBarberos::BarberoRetirado);
        }
        let antes = HorarioDto::from(&Horario::reconstruir(p.barberos.franjas(&mut cx, id).await?));
        p.barberos.reemplazar_franjas(&mut cx, id, &horario).await?;
        let despues = HorarioDto::from(&horario);
        p.publicar(&mut cx, &HorarioCambiado { barbero: id }).await?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "cambiar_horario", id)
                .antes(&antes)
                .despues(&despues),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}

caso_de_uso!(
    /// Servicios que ofrece el barbero (RF-BAR-08, RF-PBA-09).
    GestionServicios
);

impl<C: Send + 'static> GestionServicios<C> {
    pub(crate) async fn consultar(
        &self,
        actor: &Actor,
        id: IdBarbero,
    ) -> Result<Vec<ServicioOfrecidoDto>, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let servicios = p.guardado(&mut cx, id).await?.servicios;
        p.transacciones.confirmar(cx).await?;
        Ok(servicios)
    }

    pub(crate) async fn cambiar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        mut ids: Vec<IdServicio>,
    ) -> Result<Vec<ServicioOfrecidoDto>, ErrorBarberos> {
        administrador_o_propio(actor, id)?;
        ids.sort();
        ids.dedup();
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let antes = p.guardado(&mut cx, id).await?.servicios;
        let actuales: Vec<IdServicio> = antes.iter().map(|s| s.id).collect();
        p.validar_servicios(&mut cx, &ids, &actuales).await?;
        p.barberos.reemplazar_servicios(&mut cx, id, &ids).await?;
        let despues = p.guardado(&mut cx, id).await?.servicios;
        p.auditar(
            &mut cx,
            p.entrada(actor, "cambiar_servicios", id)
                .antes(&antes)
                .despues(&despues),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
