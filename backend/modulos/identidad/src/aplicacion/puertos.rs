//! Puertos del módulo, genéricos en el contexto transaccional `C` (ADR 0005): en producción
//! `C` es una transacción de PostgreSQL; en las pruebas, `()`.

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use auditoria::api::Auditor;
use chrono::{DateTime, Utc};
use notificaciones::api::BuzonSalida;
use nucleo::identificadores::IdUsuario;
use nucleo::{Actor, Correo, ErrorPersistencia, Reloj, Transacciones};
use plataforma::seguridad::Huella;

use super::dto::{AdministradorDto, CuentaPersonalDto};
use super::errores::ErrorIdentidad;
use crate::api::PerfilesBarberos;
use crate::dominio::{ContrasenaNueva, DuracionSesion, NombrePersona, PropositoToken, Rol, Usuario};

#[async_trait]
pub(crate) trait RepositorioUsuarios<C: Send>: Send + Sync + fmt::Debug {
    /// Busca por correo (sin distinguir mayúsculas) y **bloquea la fila** hasta el fin de la
    /// transacción, para contar bien los intentos fallidos simultáneos.
    async fn por_correo(&self, cx: &mut C, correo: &Correo) -> Result<Option<Usuario>, ErrorPersistencia>;

    async fn por_id(&self, cx: &mut C, id: IdUsuario) -> Result<Option<Usuario>, ErrorPersistencia>;

    /// Guarda intentos fallidos y bloqueo; con `acceso`, también la hora del último acceso.
    async fn guardar_acceso(
        &self,
        cx: &mut C,
        usuario: &Usuario,
        acceso: Option<DateTime<Utc>>,
    ) -> Result<(), ErrorPersistencia>;

    /// Cambia la contraseña y reinicia el conteo de fallos y el bloqueo.
    async fn guardar_contrasena(
        &self,
        cx: &mut C,
        id: IdUsuario,
        hash: &str,
    ) -> Result<(), ErrorPersistencia>;

    /// Crea la cuenta sin contraseña. `Duplicado` si el correo ya existe.
    async fn crear_administrador(
        &self,
        cx: &mut C,
        correo: &Correo,
        nombre: &NombrePersona,
        ahora: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia>;

    async fn listar_administradores(&self, cx: &mut C) -> Result<Vec<AdministradorDto>, ErrorPersistencia>;

    /// Administradores activos, con sus filas bloqueadas: dos desactivaciones simultáneas no
    /// pueden dejar la barbería sin administradores.
    async fn administradores_activos(&self, cx: &mut C) -> Result<Vec<IdUsuario>, ErrorPersistencia>;

    async fn cambiar_activo(&self, cx: &mut C, id: IdUsuario, activo: bool) -> Result<(), ErrorPersistencia>;

    /// Crea la cuenta de un barbero, sin contraseña. `Duplicado` si el correo ya existe.
    async fn crear_barbero(
        &self,
        cx: &mut C,
        correo: &Correo,
        activo: bool,
        ahora: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia>;

    /// `Duplicado` si el correo ya es de otra cuenta.
    async fn cambiar_correo(
        &self,
        cx: &mut C,
        id: IdUsuario,
        correo: &Correo,
    ) -> Result<(), ErrorPersistencia>;

    async fn resumen_cuenta(
        &self,
        cx: &mut C,
        id: IdUsuario,
    ) -> Result<Option<CuentaPersonalDto>, ErrorPersistencia>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuevaSesion {
    pub(crate) usuario: IdUsuario,
    pub(crate) huella: Huella,
    pub(crate) duracion: DuracionSesion,
    pub(crate) ahora: DateTime<Utc>,
    pub(crate) ip: Option<IpAddr>,
    pub(crate) agente: Option<String>,
}

/// Sesión no revocada ni vencida, con su cuenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SesionVigente {
    pub(crate) id: i64,
    pub(crate) usuario: Usuario,
    pub(crate) duracion: DuracionSesion,
    pub(crate) creada_en: DateTime<Utc>,
    pub(crate) ultimo_uso_en: DateTime<Utc>,
}

#[async_trait]
pub(crate) trait RepositorioSesiones<C: Send>: Send + Sync + fmt::Debug {
    async fn crear(&self, cx: &mut C, sesion: &NuevaSesion) -> Result<(), ErrorPersistencia>;

    async fn vigente(
        &self,
        cx: &mut C,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<SesionVigente>, ErrorPersistencia>;

    async fn renovar(
        &self,
        cx: &mut C,
        id: i64,
        ahora: DateTime<Utc>,
        expira_en: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;

    async fn revocar(
        &self,
        cx: &mut C,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;

    /// Revoca todas las sesiones de la cuenta, salvo `excepto` (la sesión actual).
    async fn revocar_de_usuario(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
        excepto: Option<&Huella>,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;
}

#[async_trait]
pub(crate) trait RepositorioTokens<C: Send>: Send + Sync + fmt::Debug {
    /// Guarda un enlace nuevo e invalida los anteriores del mismo propósito: solo sirve el último.
    async fn emitir(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
        proposito: PropositoToken,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia>;

    /// Marca el enlace como usado y devuelve su cuenta, si estaba vigente.
    async fn consumir(
        &self,
        cx: &mut C,
        huella: &Huella,
        proposito: PropositoToken,
        ahora: DateTime<Utc>,
    ) -> Result<Option<IdUsuario>, ErrorPersistencia>;
}

/// Cifrado y verificación de contraseñas (Argon2id en producción).
#[async_trait]
pub(crate) trait Credenciales: Send + Sync + fmt::Debug {
    async fn cifrar(&self, contrasena: &ContrasenaNueva) -> Result<String, ErrorIdentidad>;

    /// Sin `hash` también tarda lo mismo, para no revelar si la cuenta existe.
    async fn verificar(&self, contrasena: &str, hash: Option<&str>) -> bool;
}

/// Puertos que comparten los casos de uso del módulo.
#[derive(Debug)]
pub(crate) struct Puertos<C: Send> {
    pub(crate) transacciones: Arc<dyn Transacciones<C>>,
    pub(crate) usuarios: Arc<dyn RepositorioUsuarios<C>>,
    pub(crate) sesiones: Arc<dyn RepositorioSesiones<C>>,
    pub(crate) tokens: Arc<dyn RepositorioTokens<C>>,
    pub(crate) perfiles: Arc<dyn PerfilesBarberos<C>>,
    pub(crate) auditor: Arc<dyn Auditor<C>>,
    pub(crate) buzon: Arc<dyn BuzonSalida<C>>,
    pub(crate) credenciales: Arc<dyn Credenciales>,
    pub(crate) reloj: Arc<dyn Reloj>,
    /// URL pública del sitio, para los enlaces de los correos.
    pub(crate) url_publica: String,
}

/// Cómo se presenta una cuenta: su actor y su nombre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identificacion {
    /// `None` si es un barbero sin perfil o sin acceso (retirado o suspendido).
    pub(crate) actor: Option<Actor>,
    pub(crate) nombre: String,
}

impl<C: Send> Puertos<C> {
    /// Emite un enlace de activación (invalida los anteriores) y encola el correo de invitación.
    /// Devuelve el enlace.
    pub(crate) async fn enviar_invitacion(
        &self,
        cx: &mut C,
        usuario: IdUsuario,
        correo: &Correo,
        nombre: &str,
        rol: &str,
    ) -> Result<String, ErrorIdentidad> {
        let (token, huella) = plataforma::seguridad::generar_token()?;
        let proposito = PropositoToken::Activacion;
        self.tokens
            .emitir(cx, usuario, proposito, &huella, self.reloj.ahora())
            .await?;
        let enlace = proposito.enlace(&self.url_publica, &token);
        self.buzon
            .encolar_correo(
                cx,
                notificaciones::api::CorreoSaliente {
                    destinatario: correo.clone(),
                    plantilla: notificaciones::api::PlantillaCorreo::InvitacionPersonal {
                        nombre: nombre.to_owned(),
                        rol: rol.to_owned(),
                        enlace: enlace.clone(),
                    },
                },
            )
            .await?;
        Ok(enlace)
    }

    pub(crate) async fn identificar(
        &self,
        cx: &mut C,
        usuario: &Usuario,
    ) -> Result<Identificacion, ErrorIdentidad> {
        Ok(match usuario.rol {
            Rol::Administrador => Identificacion {
                actor: Some(Actor::Administrador { usuario: usuario.id }),
                nombre: usuario
                    .nombre_administrador
                    .clone()
                    .unwrap_or_else(|| usuario.correo.to_string()),
            },
            Rol::Barbero => match self.perfiles.por_usuario(cx, usuario.id).await? {
                Some(perfil) => Identificacion {
                    actor: perfil.puede_acceder.then_some(Actor::Barbero {
                        usuario: usuario.id,
                        barbero: perfil.barbero,
                    }),
                    nombre: perfil.nombre,
                },
                None => Identificacion {
                    actor: None,
                    nombre: usuario.correo.to_string(),
                },
            },
        })
    }
}
