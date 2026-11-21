use nucleo::identificadores::{IdBarbero, IdServicio};
use nucleo::{Actor, ErrorPersistencia};

use super::equipo::SolicitudVisibilidad;
use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::BarberoDto;
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::RESTRICCION_DOCUMENTO;
use crate::dominio::{DatosBarbero, Franja, Horario, PropuestaBarbero};

/// Datos del formulario de registro (RF-BAR-04, RF-BAR-07 a 09).
#[derive(Debug, Clone, Default)]
pub(crate) struct SolicitudRegistro {
    pub(crate) propuesta: PropuestaBarbero,
    pub(crate) servicios: Vec<IdServicio>,
    pub(crate) franjas: Vec<Franja>,
    pub(crate) visibilidad: SolicitudVisibilidad,
}

pub(crate) fn documento_en_uso(error: ErrorPersistencia) -> ErrorBarberos {
    match error {
        ErrorPersistencia::Duplicado { restriccion } if restriccion == RESTRICCION_DOCUMENTO => {
            ErrorBarberos::DocumentoEnUso
        }
        otro => otro.into(),
    }
}

/// Une los errores de varias validaciones para mostrarlos todos de una vez.
fn juntar<A, B>(
    a: Result<A, Vec<nucleo::ErrorDeCampo>>,
    b: Result<B, Vec<nucleo::ErrorDeCampo>>,
) -> Result<(A, B), ErrorBarberos> {
    match (a, b) {
        (Ok(a), Ok(b)) => Ok((a, b)),
        (a, b) => {
            let mut errores = a.err().unwrap_or_default();
            errores.extend(b.err().unwrap_or_default());
            Err(ErrorBarberos::DatosInvalidos(errores))
        }
    }
}

caso_de_uso!(
    /// Registra un barbero con sus datos esenciales, horario, servicios y visibilidad. Crea su
    /// cuenta (el correo es su usuario) y, si tiene acceso al panel, le envía la invitación.
    /// Queda al final del orden del equipo.
    RegistrarBarbero
);

impl<C: Send + 'static> RegistrarBarbero<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        s: SolicitudRegistro,
    ) -> Result<BarberoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let comision = p.parametros.comision_por_defecto(&mut cx).await?;
        let (datos, horario) = juntar(
            DatosBarbero::nuevo(s.propuesta, p.hoy(), comision),
            Horario::nuevo(s.franjas),
        )?;
        let mut servicios = s.servicios;
        servicios.sort();
        servicios.dedup();
        p.validar_servicios(&mut cx, &servicios, &[]).await?;

        let nombre = datos.nombre_completo();
        let usuario = p
            .cuentas
            .crear_cuenta_barbero(&mut cx, actor, &datos.correo, &nombre, s.visibilidad.acceso_panel)
            .await?;
        let id = p
            .barberos
            .crear(
                &mut cx,
                usuario,
                &datos,
                s.visibilidad.mostrar_en_landing,
                s.visibilidad.recibe_citas_en_linea,
            )
            .await
            .map_err(documento_en_uso)?;
        p.barberos.reemplazar_franjas(&mut cx, id, &horario).await?;
        p.barberos.reemplazar_servicios(&mut cx, id, &servicios).await?;
        let ficha = p.ficha(&mut cx, id).await?;
        p.auditar(&mut cx, p.entrada(actor, "registrar_barbero", id).despues(&ficha))
            .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(ficha)
    }
}

caso_de_uso!(
    /// Edita los datos esenciales (RF-BAR-04). Si cambia el correo, cambia también el usuario
    /// del panel. Queda auditado con el antes y el después (RF-BAR-16).
    EditarBarbero
);

impl<C: Send + 'static> EditarBarbero<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        propuesta: PropuestaBarbero,
    ) -> Result<BarberoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let antes = p.ficha(&mut cx, id).await?;
        // Sin comisión en la solicitud, se conserva la actual.
        let datos = DatosBarbero::nuevo(propuesta, p.hoy(), antes.porcentaje_comision)?;
        let correo_actual = antes
            .cuenta
            .as_ref()
            .map(|c| c.correo.clone())
            .unwrap_or_default();
        if !correo_actual.eq_ignore_ascii_case(datos.correo.como_str()) {
            p.cuentas
                .cambiar_correo(
                    &mut cx,
                    actor,
                    antes.usuario_id,
                    &datos.correo,
                    &datos.nombre_completo(),
                )
                .await?;
        }
        p.barberos
            .actualizar(&mut cx, id, &datos)
            .await
            .map_err(documento_en_uso)?;
        let despues = p.ficha(&mut cx, id).await?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "editar_barbero", id)
                .antes(&antes)
                .despues(&despues),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}
