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

/// Imprime un enlace de acceso para una cuenta del personal, sin enviar correo: de activación
/// si no la ha activado (p. ej. un barbero que no recibió la invitación) o de restablecimiento
/// si ya la activó. Requiere acceso al servidor; queda en la auditoría.
#[allow(
    clippy::print_stdout,
    reason = "es la salida del comando para la persona que lo ejecuta"
)]
pub(crate) async fn enlace_acceso(
    configuracion: &Configuracion,
    correo: &str,
) -> Result<(), ErrorDeArranque> {
    let composicion = Composicion::construir(configuracion).await?;
    let enlace = composicion.identidad.enlace_de_acceso(correo).await?;
    println!("Enlace de acceso para {correo} (un solo uso; no se envió por correo):");
    println!("{enlace}");
    composicion.pool.close().await;
    Ok(())
}
