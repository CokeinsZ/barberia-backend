use std::net::IpAddr;
use std::sync::Arc;

use auditoria::api::EntradaAuditoria;
use nucleo::{Actor, Correo};
use plataforma::seguridad::generar_token;

use super::{ENTIDAD_USUARIO, MODULO};
use crate::aplicacion::dto::{SesionDto, UsuarioSesionDto};
use crate::aplicacion::errores::ErrorIdentidad;
use crate::aplicacion::puertos::{NuevaSesion, Puertos};
use crate::dominio::DuracionSesion;
use crate::dominio::usuario::EfectoFallo;

#[derive(Debug, Clone)]
pub(crate) struct SolicitudInicioSesion {
    pub(crate) correo: String,
    pub(crate) contrasena: String,
    pub(crate) recordarme: bool,
    pub(crate) ip: Option<IpAddr>,
    pub(crate) agente: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SesionIniciada {
    /// Token para la cookie. Solo existe aquí: la base guarda su huella.
    pub(crate) token: String,
    pub(crate) duracion: DuracionSesion,
    pub(crate) sesion: SesionDto,
}

/// RF-AUT-01 y RF-AUT-04. Bloquea la cuenta 15 minutos tras 5 fallos (RNF-SEG-05).
#[derive(Debug)]
pub(crate) struct IniciarSesion<C: Send> {
    p: Arc<Puertos<C>>,
}

impl<C: Send> IniciarSesion<C> {
    pub(crate) fn nuevo(p: Arc<Puertos<C>>) -> Self {
        Self { p }
    }

    pub(crate) async fn ejecutar(
        &self,
        solicitud: SolicitudInicioSesion,
    ) -> Result<SesionIniciada, ErrorIdentidad> {
        let p = &self.p;
        let ahora = p.reloj.ahora();
        let Ok(correo) = Correo::nuevo(&solicitud.correo) else {
            p.credenciales.verificar(&solicitud.contrasena, None).await;
            return Err(ErrorIdentidad::CredencialesInvalidas);
        };

        let mut cx = p.transacciones.iniciar().await?;
        let Some(mut usuario) = p.usuarios.por_correo(&mut cx, &correo).await? else {
            // Misma demora que con una cuenta real: el tiempo de respuesta no revela si existe.
            p.credenciales.verificar(&solicitud.contrasena, None).await;
            return Err(ErrorIdentidad::CredencialesInvalidas);
        };
        if usuario.esta_bloqueado(ahora) {
            p.credenciales.verificar(&solicitud.contrasena, None).await;
            return Err(ErrorIdentidad::CredencialesInvalidas);
        }

        if !p
            .credenciales
            .verificar(&solicitud.contrasena, usuario.hash_contrasena.as_deref())
            .await
        {
            if usuario.registrar_fallo(ahora) == EfectoFallo::Bloqueada {
                tracing::warn!(usuario = %usuario.id, "cuenta bloqueada por intentos fallidos");
                p.auditor
                    .registrar(
                        &mut cx,
                        EntradaAuditoria::nueva(
                            &Actor::Sistema,
                            MODULO,
                            "bloquear_cuenta",
                            ENTIDAD_USUARIO,
                            usuario.id,
                        )
                        .despues(&usuario.bloqueado_hasta),
                    )
                    .await?;
            }
            p.usuarios.guardar_acceso(&mut cx, &usuario, None).await?;
            p.transacciones.confirmar(cx).await?;
            return Err(ErrorIdentidad::CredencialesInvalidas);
        }

        let identificacion = p.identificar(&mut cx, &usuario).await?;
        if !usuario.activo || identificacion.actor.is_none() {
            return Err(ErrorIdentidad::AccesoSuspendido);
        }

        usuario.registrar_exito();
        p.usuarios.guardar_acceso(&mut cx, &usuario, Some(ahora)).await?;
        let (token, huella) = generar_token()?;
        let duracion = DuracionSesion::desde_recordarme(solicitud.recordarme);
        p.sesiones
            .crear(
                &mut cx,
                &NuevaSesion {
                    usuario: usuario.id,
                    huella,
                    duracion,
                    ahora,
                    ip: solicitud.ip,
                    agente: solicitud.agente.map(|a| a.chars().take(300).collect()),
                },
            )
            .await?;
        p.transacciones.confirmar(cx).await?;

        Ok(SesionIniciada {
            token,
            duracion,
            sesion: SesionDto {
                usuario: UsuarioSesionDto {
                    id: usuario.id,
                    rol: usuario.rol.como_str(),
                    nombre: identificacion.nombre,
                    correo: usuario.correo.to_string(),
                },
                panel: usuario.rol.panel(),
            },
        })
    }
}
