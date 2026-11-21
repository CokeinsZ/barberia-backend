use std::sync::Arc;

use nucleo::Actor;

use crate::aplicacion::dto::HorarioDiaDto;
use crate::aplicacion::errores::ErrorNegocio;
use crate::aplicacion::politicas;
use crate::aplicacion::puertos::Puertos;

const DIAS: [&str; 7] = [
    "Lunes",
    "Martes",
    "Miércoles",
    "Jueves",
    "Viernes",
    "Sábado",
    "Domingo",
];

/// Horario de atención por día de la semana (RF-CFG-03): no se configura, se calcula con las
/// franjas de los barberos vigentes, desde la más temprana hasta la más tardía (DA-10, RN-04).
#[derive(Debug)]
pub(crate) struct ConsultarHorarioAtencion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> ConsultarHorarioAtencion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(&self, actor: &Actor) -> Result<Vec<HorarioDiaDto>, ErrorNegocio> {
        politicas::solo_personal(actor)?;
        let mut cx = self.p.transacciones.iniciar().await?;
        let horario = calcular(&self.p, &mut cx).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(horario)
    }
}

/// Los 7 días con su apertura y cierre (lo usan el panel y la landing).
pub(crate) async fn calcular<C: Send>(
    p: &Puertos<C>,
    cx: &mut C,
) -> Result<Vec<HorarioDiaDto>, ErrorNegocio> {
    let franjas = p.horario.por_dia(cx).await?;
    Ok((1_u8..=7)
        .map(|dia| {
            let franja = franjas.iter().find(|(d, _, _)| *d == i16::from(dia));
            HorarioDiaDto {
                dia_semana: dia,
                nombre: DIAS[usize::from(dia - 1)],
                abierto: franja.is_some(),
                abre: franja.map(|f| f.1),
                cierra: franja.map(|f| f.2),
            }
        })
        .collect())
}
