//! Comandos de administración por línea de comandos.

use nucleo::Actor;
use plataforma::configuracion::Configuracion;

use crate::ErrorDeArranque;
use crate::composicion::Composicion;

/// Invita a un administrador con el actor `Sistema` (queda así en la auditoría). El correo
/// queda en cola como cualquier invitación, y además el enlace se imprime para poder activar
/// la cuenta aunque el SMTP aún no esté configurado.
#[allow(
    clippy::print_stdout,
    reason = "es la salida del comando para la persona que lo ejecuta"
)]
pub(crate) async fn crear_administrador(
    configuracion: &Configuracion,
    correo: &str,
    nombre: &str,
) -> Result<(), ErrorDeArranque> {
    let composicion = Composicion::construir(configuracion).await?;
    let invitacion = composicion
        .identidad
        .invitar_administrador(&Actor::Sistema, correo, nombre)
        .await?;
    println!("Administrador invitado (id {}).", invitacion.usuario);
    println!("Enlace para crear la contraseña (vence en 7 días, un solo uso):");
    println!("{}", invitacion.enlace);
    composicion.pool.close().await;
    Ok(())
}
