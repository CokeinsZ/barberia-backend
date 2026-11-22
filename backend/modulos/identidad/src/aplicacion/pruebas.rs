//! Pruebas de los casos de uso con dobles en memoria (contexto `C = ()`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use auditoria::api::{Auditor, EntradaAuditoria};
use chrono::{DateTime, TimeDelta, Utc};
use notificaciones::api::{BuzonSalida, CorreoSaliente, PlantillaCorreo, WhatsAppSaliente};
use nucleo::identificadores::{IdBarbero, IdUsuario};
use nucleo::{Actor, Correo, ErrorPersistencia, RelojControlado, SinTransaccion};
use plataforma::seguridad::{Huella, huella_token};

use super::CasosDeUso;
use super::comandos::SolicitudInicioSesion;
use super::dto::{AdministradorDto, CuentaPersonalDto};
use super::errores::ErrorIdentidad;
use super::puertos::{
    Credenciales, NuevaSesion, Puertos, RepositorioSesiones, RepositorioTokens, RepositorioUsuarios,
    SesionVigente,
};
use crate::api::{PerfilBarbero, PerfilesBarberos};
use crate::dominio::{ContrasenaNueva, DuracionSesion, NombrePersona, PropositoToken, Rol, Usuario};

// ---------------------------------------------------------------------------------------------
// Dobles
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Usuarios(Mutex<Vec<(Usuario, Option<DateTime<Utc>>)>>);

#[async_trait]
impl RepositorioUsuarios<()> for Usuarios {
    async fn por_correo(&self, _: &mut (), correo: &Correo) -> Result<Option<Usuario>, ErrorPersistencia> {
        let buscado = correo.como_str().to_lowercase();
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|(u, _)| u.correo.como_str().to_lowercase() == buscado)
            .map(|(u, _)| u.clone()))
    }
    async fn por_id(&self, _: &mut (), id: IdUsuario) -> Result<Option<Usuario>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|(u, _)| u.id == id)
            .map(|(u, _)| u.clone()))
    }
    async fn guardar_acceso(
        &self,
        _: &mut (),
        usuario: &Usuario,
        acceso: Option<DateTime<Utc>>,
    ) -> Result<(), ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        let fila = filas.iter_mut().find(|(u, _)| u.id == usuario.id).unwrap();
        fila.0.intentos_fallidos = usuario.intentos_fallidos;
        fila.0.bloqueado_hasta = usuario.bloqueado_hasta;
        fila.1 = acceso.or(fila.1);
        Ok(())
    }
    async fn guardar_contrasena(
        &self,
        _: &mut (),
        id: IdUsuario,
        hash: &str,
    ) -> Result<(), ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        let (u, _) = filas.iter_mut().find(|(u, _)| u.id == id).unwrap();
        u.hash_contrasena = Some(hash.into());
        u.intentos_fallidos = 0;
        u.bloqueado_hasta = None;
        Ok(())
    }
    async fn crear_administrador(
        &self,
        _: &mut (),
        correo: &Correo,
        nombre: &NombrePersona,
        _: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        if filas
            .iter()
            .any(|(u, _)| u.correo.como_str().eq_ignore_ascii_case(correo.como_str()))
        {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: "usuario_correo_uk".into(),
            });
        }
        let id = IdUsuario(i64::try_from(filas.len()).unwrap() + 1);
        filas.push((
            Usuario {
                id,
                correo: correo.clone(),
                rol: Rol::Administrador,
                nombre_administrador: Some(nombre.como_str().into()),
                hash_contrasena: None,
                activo: true,
                intentos_fallidos: 0,
                bloqueado_hasta: None,
            },
            None,
        ));
        Ok(id)
    }
    async fn listar_administradores(&self, _: &mut ()) -> Result<Vec<AdministradorDto>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, _)| u.rol == Rol::Administrador)
            .map(|(u, acceso)| AdministradorDto {
                id: u.id,
                correo: u.correo.to_string(),
                nombre: u.nombre_administrador.clone().unwrap_or_default(),
                activo: u.activo,
                pendiente_activacion: u.hash_contrasena.is_none(),
                ultimo_acceso_en: *acceso,
            })
            .collect())
    }
    async fn administradores_activos(&self, _: &mut ()) -> Result<Vec<IdUsuario>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, _)| u.rol == Rol::Administrador && u.activo)
            .map(|(u, _)| u.id)
            .collect())
    }
    async fn cambiar_activo(&self, _: &mut (), id: IdUsuario, activo: bool) -> Result<(), ErrorPersistencia> {
        self.0
            .lock()
            .unwrap()
            .iter_mut()
            .find(|(u, _)| u.id == id)
            .unwrap()
            .0
            .activo = activo;
        Ok(())
    }
    async fn crear_barbero(
        &self,
        _: &mut (),
        correo: &Correo,
        activo: bool,
        _: DateTime<Utc>,
    ) -> Result<IdUsuario, ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        if filas
            .iter()
            .any(|(u, _)| u.correo.como_str().eq_ignore_ascii_case(correo.como_str()))
        {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: "usuario_correo_uk".into(),
            });
        }
        let id = IdUsuario(i64::try_from(filas.len()).unwrap() + 1);
        filas.push((
            Usuario {
                id,
                correo: correo.clone(),
                rol: Rol::Barbero,
                nombre_administrador: None,
                hash_contrasena: None,
                activo,
                intentos_fallidos: 0,
                bloqueado_hasta: None,
            },
            None,
        ));
        Ok(id)
    }
    async fn cambiar_correo(
        &self,
        _: &mut (),
        id: IdUsuario,
        correo: &Correo,
    ) -> Result<(), ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        if filas
            .iter()
            .any(|(u, _)| u.id != id && u.correo.como_str().eq_ignore_ascii_case(correo.como_str()))
        {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: "usuario_correo_uk".into(),
            });
        }
        filas.iter_mut().find(|(u, _)| u.id == id).unwrap().0.correo = correo.clone();
        Ok(())
    }
    async fn resumen_cuenta(
        &self,
        _: &mut (),
        id: IdUsuario,
    ) -> Result<Option<CuentaPersonalDto>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|(u, _)| u.id == id)
            .map(|(u, acceso)| CuentaPersonalDto {
                usuario: u.id,
                correo: u.correo.to_string(),
                acceso_panel: u.activo,
                pendiente_activacion: u.hash_contrasena.is_none(),
                ultimo_acceso_en: *acceso,
            }))
    }
}

#[derive(Debug, Clone)]
struct FilaSesion {
    id: i64,
    usuario: IdUsuario,
    huella: Huella,
    duracion: DuracionSesion,
    creada: DateTime<Utc>,
    ultimo_uso: DateTime<Utc>,
    expira: DateTime<Utc>,
    revocada: bool,
}

#[derive(Debug)]
struct Sesiones {
    filas: Mutex<Vec<FilaSesion>>,
    usuarios: Arc<Usuarios>,
}

#[async_trait]
impl RepositorioSesiones<()> for Sesiones {
    async fn crear(&self, _: &mut (), s: &NuevaSesion) -> Result<(), ErrorPersistencia> {
        let mut filas = self.filas.lock().unwrap();
        let id = i64::try_from(filas.len()).unwrap() + 1;
        filas.push(FilaSesion {
            id,
            usuario: s.usuario,
            huella: s.huella,
            duracion: s.duracion,
            creada: s.ahora,
            ultimo_uso: s.ahora,
            expira: s.duracion.expira_en(s.ahora),
            revocada: false,
        });
        Ok(())
    }
    async fn vigente(
        &self,
        cx: &mut (),
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<Option<SesionVigente>, ErrorPersistencia> {
        let fila = self
            .filas
            .lock()
            .unwrap()
            .iter()
            .find(|f| f.huella == *huella && !f.revocada && f.expira > ahora)
            .cloned();
        let Some(f) = fila else { return Ok(None) };
        let usuario = self.usuarios.por_id(cx, f.usuario).await?.unwrap();
        Ok(Some(SesionVigente {
            id: f.id,
            usuario,
            duracion: f.duracion,
            creada_en: f.creada,
            ultimo_uso_en: f.ultimo_uso,
        }))
    }
    async fn renovar(
        &self,
        _: &mut (),
        id: i64,
        ahora: DateTime<Utc>,
        expira: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        let mut filas = self.filas.lock().unwrap();
        let f = filas.iter_mut().find(|f| f.id == id).unwrap();
        f.ultimo_uso = ahora;
        f.expira = expira;
        Ok(())
    }
    async fn revocar(&self, _: &mut (), huella: &Huella, _: DateTime<Utc>) -> Result<(), ErrorPersistencia> {
        for f in self
            .filas
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|f| f.huella == *huella)
        {
            f.revocada = true;
        }
        Ok(())
    }
    async fn revocar_de_usuario(
        &self,
        _: &mut (),
        usuario: IdUsuario,
        excepto: Option<&Huella>,
        _: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        for f in self
            .filas
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|f| f.usuario == usuario && Some(&f.huella) != excepto)
        {
            f.revocada = true;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct FilaToken {
    usuario: IdUsuario,
    proposito: PropositoToken,
    huella: Huella,
    expira: DateTime<Utc>,
    usado: bool,
}

#[derive(Debug, Default)]
struct Tokens(Mutex<Vec<FilaToken>>);

#[async_trait]
impl RepositorioTokens<()> for Tokens {
    async fn emitir(
        &self,
        _: &mut (),
        usuario: IdUsuario,
        proposito: PropositoToken,
        huella: &Huella,
        ahora: DateTime<Utc>,
    ) -> Result<(), ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        for t in filas
            .iter_mut()
            .filter(|t| t.usuario == usuario && t.proposito == proposito)
        {
            t.usado = true;
        }
        filas.push(FilaToken {
            usuario,
            proposito,
            huella: *huella,
            expira: ahora + proposito.vigencia(),
            usado: false,
        });
        Ok(())
    }
    async fn consumir(
        &self,
        _: &mut (),
        huella: &Huella,
        proposito: PropositoToken,
        ahora: DateTime<Utc>,
    ) -> Result<Option<IdUsuario>, ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        let Some(t) = filas
            .iter_mut()
            .find(|t| t.huella == *huella && t.proposito == proposito && !t.usado && t.expira > ahora)
        else {
            return Ok(None);
        };
        t.usado = true;
        Ok(Some(t.usuario))
    }
}

#[derive(Debug, Default)]
struct Perfiles(Mutex<HashMap<IdUsuario, PerfilBarbero>>);

#[async_trait]
impl PerfilesBarberos<()> for Perfiles {
    async fn por_usuario(
        &self,
        _: &mut (),
        usuario: IdUsuario,
    ) -> Result<Option<PerfilBarbero>, ErrorPersistencia> {
        Ok(self.0.lock().unwrap().get(&usuario).cloned())
    }
}

#[derive(Debug, Default)]
struct Auditoria(Mutex<Vec<(&'static str, String)>>);

#[async_trait]
impl Auditor<()> for Auditoria {
    async fn registrar(&self, _: &mut (), entrada: EntradaAuditoria) -> Result<(), ErrorPersistencia> {
        self.0.lock().unwrap().push((entrada.accion, entrada.entidad_id));
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Buzon(Mutex<Vec<CorreoSaliente>>);

#[async_trait]
impl BuzonSalida<()> for Buzon {
    async fn encolar_correo(&self, _: &mut (), correo: CorreoSaliente) -> Result<(), ErrorPersistencia> {
        self.0.lock().unwrap().push(correo);
        Ok(())
    }
    async fn encolar_whatsapp(&self, _: &mut (), _: WhatsAppSaliente) -> Result<i64, ErrorPersistencia> {
        Ok(0)
    }
}

/// «Cifra» anteponiendo un prefijo: las pruebas no necesitan la lentitud de Argon2.
#[derive(Debug)]
struct CredencialesFalsas;

#[async_trait]
impl Credenciales for CredencialesFalsas {
    async fn cifrar(&self, contrasena: &ContrasenaNueva) -> Result<String, ErrorIdentidad> {
        Ok(format!("hash:{}", contrasena.exponer()))
    }
    async fn verificar(&self, contrasena: &str, hash: Option<&str>) -> bool {
        hash == Some(format!("hash:{contrasena}").as_str())
    }
}

// ---------------------------------------------------------------------------------------------
// Escenario
// ---------------------------------------------------------------------------------------------

const CLAVE: &str = "una clave bien larga";

struct Escenario {
    casos: CasosDeUso<()>,
    usuarios: Arc<Usuarios>,
    sesiones: Arc<Sesiones>,
    perfiles: Arc<Perfiles>,
    auditoria: Arc<Auditoria>,
    buzon: Arc<Buzon>,
    reloj: Arc<RelojControlado>,
}

impl Escenario {
    /// Laura (administradora, id 1) y Santiago (barbero, id 2), ambos activados.
    fn nuevo() -> Self {
        let usuarios = Arc::new(Usuarios::default());
        let base = |id, correo: &str, rol, nombre: Option<&str>| {
            (
                Usuario {
                    id: IdUsuario(id),
                    correo: Correo::nuevo(correo).unwrap(),
                    rol,
                    nombre_administrador: nombre.map(Into::into),
                    hash_contrasena: Some(format!("hash:{CLAVE}")),
                    activo: true,
                    intentos_fallidos: 0,
                    bloqueado_hasta: None,
                },
                None,
            )
        };
        usuarios.0.lock().unwrap().extend([
            base(1, "laura@diamante.co", Rol::Administrador, Some("Laura Gómez")),
            base(2, "santiago@diamante.co", Rol::Barbero, None),
        ]);
        let perfiles = Arc::new(Perfiles::default());
        perfiles.0.lock().unwrap().insert(
            IdUsuario(2),
            PerfilBarbero {
                barbero: IdBarbero(7),
                nombre: "Santiago Ruiz".into(),
                puede_acceder: true,
            },
        );
        let sesiones = Arc::new(Sesiones {
            filas: Mutex::default(),
            usuarios: usuarios.clone(),
        });
        let auditoria = Arc::new(Auditoria::default());
        let buzon = Arc::new(Buzon::default());
        let reloj = Arc::new(RelojControlado::en("2026-11-20T14:00:00Z".parse().unwrap()));
        let casos = CasosDeUso::nuevos(Puertos {
            transacciones: Arc::new(SinTransaccion),
            usuarios: usuarios.clone(),
            sesiones: sesiones.clone(),
            tokens: Arc::new(Tokens::default()),
            perfiles: perfiles.clone(),
            auditor: auditoria.clone(),
            buzon: buzon.clone(),
            credenciales: Arc::new(CredencialesFalsas),
            reloj: reloj.clone(),
            url_publica: "https://diamante.co".into(),
        });
        Self {
            casos,
            usuarios,
            sesiones,
            perfiles,
            auditoria,
            buzon,
            reloj,
        }
    }

    async fn entrar(&self, correo: &str, contrasena: &str) -> Result<String, ErrorIdentidad> {
        self.entrar_con(correo, contrasena, false).await
    }

    async fn entrar_con(
        &self,
        correo: &str,
        contrasena: &str,
        recordarme: bool,
    ) -> Result<String, ErrorIdentidad> {
        let solicitud = SolicitudInicioSesion {
            correo: correo.into(),
            contrasena: contrasena.into(),
            recordarme,
            ip: None,
            agente: None,
        };
        Ok(self.casos.iniciar_sesion.ejecutar(solicitud).await?.token)
    }

    async fn actor(&self, token: &str) -> Option<Actor> {
        self.casos.validar_sesion.ejecutar(token).await.unwrap()
    }

    fn adelantar(&self, delta: TimeDelta) {
        self.reloj.adelantar(delta);
    }

    /// Token del último enlace enviado por correo.
    fn token_del_ultimo_correo(&self) -> String {
        let correos = self.buzon.0.lock().unwrap();
        let enlace = match &correos.last().unwrap().plantilla {
            PlantillaCorreo::InvitacionPersonal { enlace, .. }
            | PlantillaCorreo::RecuperacionContrasena { enlace, .. } => enlace.clone(),
        };
        enlace.split_once("#token=").unwrap().1.to_owned()
    }

    fn auditado(&self, accion: &str) -> bool {
        self.auditoria.0.lock().unwrap().iter().any(|(a, _)| *a == accion)
    }
}

const LAURA: Actor = Actor::Administrador {
    usuario: IdUsuario(1),
};

// ---------------------------------------------------------------------------------------------
// Inicio de sesión
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cada_rol_llega_a_su_panel_y_su_sesion_identifica_al_actor() {
    let e = Escenario::nuevo();
    let iniciada = e
        .casos
        .iniciar_sesion
        .ejecutar(SolicitudInicioSesion {
            correo: "LAURA@diamante.co".into(),
            contrasena: CLAVE.into(),
            recordarme: false,
            ip: None,
            agente: None,
        })
        .await
        .unwrap();
    assert_eq!(iniciada.sesion.panel, "/admin");
    assert_eq!(iniciada.sesion.usuario.nombre, "Laura Gómez");
    assert_eq!(e.actor(&iniciada.token).await, Some(LAURA));

    let token = e.entrar("santiago@diamante.co", CLAVE).await.unwrap();
    assert_eq!(
        e.actor(&token).await,
        Some(Actor::Barbero {
            usuario: IdUsuario(2),
            barbero: IdBarbero(7)
        })
    );
    let sesion = e
        .casos
        .consultar_sesion
        .ejecutar(&e.actor(&token).await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        (sesion.panel, sesion.usuario.nombre.as_str()),
        ("/barbero", "Santiago Ruiz")
    );
}

#[tokio::test]
async fn un_correo_desconocido_o_una_clave_errada_dan_el_mismo_error() {
    let e = Escenario::nuevo();
    for (correo, clave) in [
        ("nadie@diamante.co", CLAVE),
        ("laura@diamante.co", "otra clave larga"),
        ("no-es-correo", CLAVE),
    ] {
        assert!(matches!(
            e.entrar(correo, clave).await,
            Err(ErrorIdentidad::CredencialesInvalidas)
        ));
    }
}

#[tokio::test]
async fn cinco_fallos_bloquean_la_cuenta_quince_minutos() {
    let e = Escenario::nuevo();
    for _ in 0..5 {
        assert!(e.entrar("laura@diamante.co", "clave equivocada!").await.is_err());
    }
    assert!(e.auditado("bloquear_cuenta"));
    // Bloqueada: ni la clave correcta sirve, y el mensaje sigue siendo genérico.
    assert!(matches!(
        e.entrar("laura@diamante.co", CLAVE).await,
        Err(ErrorIdentidad::CredencialesInvalidas)
    ));
    e.adelantar(TimeDelta::minutes(15));
    assert!(e.entrar("laura@diamante.co", CLAVE).await.is_ok());
}

#[tokio::test]
async fn un_barbero_suspendido_no_entra_y_pierde_su_sesion() {
    let e = Escenario::nuevo();
    let token = e.entrar("santiago@diamante.co", CLAVE).await.unwrap();
    e.perfiles
        .0
        .lock()
        .unwrap()
        .get_mut(&IdUsuario(2))
        .unwrap()
        .puede_acceder = false;

    assert_eq!(e.actor(&token).await, None);
    assert!(matches!(
        e.entrar("santiago@diamante.co", CLAVE).await,
        Err(ErrorIdentidad::AccesoSuspendido)
    ));
}

#[tokio::test]
async fn la_sesion_vence_tras_12_horas_sin_uso_y_cada_uso_la_extiende() {
    let e = Escenario::nuevo();
    let token = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    e.adelantar(TimeDelta::hours(11));
    assert!(e.actor(&token).await.is_some());
    e.adelantar(TimeDelta::hours(11));
    assert!(e.actor(&token).await.is_some(), "el uso anterior la extendió");
    e.adelantar(TimeDelta::hours(12));
    assert_eq!(e.actor(&token).await, None);

    let recordada = e.entrar_con("laura@diamante.co", CLAVE, true).await.unwrap();
    e.adelantar(TimeDelta::days(29));
    assert!(e.actor(&recordada).await.is_some());
}

#[tokio::test]
async fn ni_usandola_a_diario_una_sesion_dura_mas_de_30_dias() {
    let e = Escenario::nuevo();
    let recordada = e.entrar_con("laura@diamante.co", CLAVE, true).await.unwrap();
    for dia in 1..30 {
        e.adelantar(TimeDelta::days(1));
        assert!(e.actor(&recordada).await.is_some(), "día {dia}");
    }
    e.adelantar(TimeDelta::hours(23));
    assert!(e.actor(&recordada).await.is_some(), "aún dentro de los 30 días");
    e.adelantar(TimeDelta::hours(2));
    assert_eq!(
        e.actor(&recordada).await,
        None,
        "vida máxima: hay que volver a entrar"
    );

    // La normal también: aunque se use cada 11 horas, a los 30 días vence.
    let normal = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    for _ in 0..65 {
        e.adelantar(TimeDelta::hours(11));
        assert!(e.actor(&normal).await.is_some());
    }
    e.adelantar(TimeDelta::hours(11));
    assert_eq!(e.actor(&normal).await, None);
}

#[tokio::test]
async fn cerrar_sesion_invalida_el_token() {
    let e = Escenario::nuevo();
    let token = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    e.casos.cerrar_sesion.ejecutar(Some(&token)).await.unwrap();
    assert_eq!(e.actor(&token).await, None);
    e.casos.cerrar_sesion.ejecutar(None).await.unwrap();
}

// ---------------------------------------------------------------------------------------------
// Invitación, recuperación y cambio de contraseña
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn la_invitacion_permite_crear_la_contrasena_una_sola_vez() {
    let e = Escenario::nuevo();
    let invitacion = e
        .casos
        .invitar_administrador
        .ejecutar(&LAURA, "camila@diamante.co", " Camila  Rojas ")
        .await
        .unwrap();
    assert!(
        invitacion
            .enlace
            .starts_with("https://diamante.co/acceso/activar#token=")
    );
    assert!(e.auditado("invitar_administrador"));
    let token = e.token_del_ultimo_correo();

    // Una contraseña corta no gasta el enlace.
    let corta = e.casos.activar_cuenta.ejecutar(&token, "corta".into()).await;
    assert!(matches!(corta, Err(ErrorIdentidad::DatosInvalidos(ref c)) if c[0].campo == "contrasena"));
    assert!(matches!(
        e.entrar("camila@diamante.co", CLAVE).await,
        Err(ErrorIdentidad::CredencialesInvalidas)
    ));

    e.casos
        .activar_cuenta
        .ejecutar(&token, CLAVE.into())
        .await
        .unwrap();
    assert!(e.entrar("camila@diamante.co", CLAVE).await.is_ok());
    assert!(matches!(
        e.casos.activar_cuenta.ejecutar(&token, CLAVE.into()).await,
        Err(ErrorIdentidad::EnlaceVencido)
    ));
}

#[tokio::test]
async fn la_invitacion_vence_a_los_siete_dias_y_no_admite_correos_repetidos() {
    let e = Escenario::nuevo();
    e.casos
        .invitar_administrador
        .ejecutar(&Actor::Sistema, "camila@diamante.co", "Camila")
        .await
        .unwrap();
    let token = e.token_del_ultimo_correo();
    e.adelantar(TimeDelta::days(7));
    assert!(matches!(
        e.casos.activar_cuenta.ejecutar(&token, CLAVE.into()).await,
        Err(ErrorIdentidad::EnlaceVencido)
    ));

    assert!(matches!(
        e.casos
            .invitar_administrador
            .ejecutar(&LAURA, "Camila@diamante.co", "Otra")
            .await,
        Err(ErrorIdentidad::CorreoEnUso)
    ));
    let barbero = Actor::Barbero {
        usuario: IdUsuario(2),
        barbero: IdBarbero(7),
    };
    assert!(matches!(
        e.casos
            .invitar_administrador
            .ejecutar(&barbero, "x@diamante.co", "Equis")
            .await,
        Err(ErrorIdentidad::Prohibido)
    ));
}

#[tokio::test]
async fn la_recuperacion_solo_envia_correo_a_cuentas_existentes_y_el_ultimo_enlace_es_el_valido() {
    let e = Escenario::nuevo();
    e.casos
        .solicitar_recuperacion
        .ejecutar("nadie@diamante.co")
        .await
        .unwrap();
    assert!(e.buzon.0.lock().unwrap().is_empty());

    e.casos
        .solicitar_recuperacion
        .ejecutar("laura@diamante.co")
        .await
        .unwrap();
    let primero = e.token_del_ultimo_correo();
    e.casos
        .solicitar_recuperacion
        .ejecutar("laura@diamante.co")
        .await
        .unwrap();
    let segundo = e.token_del_ultimo_correo();
    assert!(matches!(
        e.casos
            .restablecer_contrasena
            .ejecutar(&primero, "clave nueva y larga".into())
            .await,
        Err(ErrorIdentidad::EnlaceVencido)
    ));

    let sesion = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    e.casos
        .restablecer_contrasena
        .ejecutar(&segundo, "clave nueva y larga".into())
        .await
        .unwrap();
    assert_eq!(
        e.actor(&sesion).await,
        None,
        "restablecer cierra todas las sesiones"
    );
    assert!(e.entrar("laura@diamante.co", "clave nueva y larga").await.is_ok());
    assert!(e.auditado("restablecer_contrasena"));
}

#[tokio::test]
async fn el_enlace_de_recuperacion_vence_en_una_hora() {
    let e = Escenario::nuevo();
    e.casos
        .solicitar_recuperacion
        .ejecutar("laura@diamante.co")
        .await
        .unwrap();
    let token = e.token_del_ultimo_correo();
    e.adelantar(TimeDelta::minutes(61));
    assert!(matches!(
        e.casos
            .restablecer_contrasena
            .ejecutar(&token, "clave nueva y larga".into())
            .await,
        Err(ErrorIdentidad::EnlaceVencido)
    ));
}

#[tokio::test]
async fn cambiar_la_contrasena_exige_la_actual_y_cierra_las_demas_sesiones() {
    let e = Escenario::nuevo();
    let otra = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    let actual = e.entrar("laura@diamante.co", CLAVE).await.unwrap();
    let huella = huella_token(&actual);

    let errada = e
        .casos
        .cambiar_contrasena
        .ejecutar(
            &LAURA,
            Some(&huella),
            "no es la clave",
            "clave nueva y larga".into(),
        )
        .await;
    assert!(matches!(errada, Err(ErrorIdentidad::DatosInvalidos(ref c)) if c[0].campo == "actual"));

    e.casos
        .cambiar_contrasena
        .ejecutar(&LAURA, Some(&huella), CLAVE, "clave nueva y larga".into())
        .await
        .unwrap();
    assert!(e.actor(&actual).await.is_some());
    assert_eq!(e.actor(&otra).await, None);
    assert_eq!(
        e.sesiones
            .filas
            .lock()
            .unwrap()
            .iter()
            .filter(|s| !s.revocada)
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// Administradores
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn nunca_queda_la_barberia_sin_administradores() {
    let e = Escenario::nuevo();
    assert!(matches!(
        e.casos
            .cambiar_estado_administrador
            .ejecutar(&LAURA, IdUsuario(1), false)
            .await,
        Err(ErrorIdentidad::NoPuedeDesactivarse)
    ));
    // Otro administrador (ya desactivado en la base, p. ej. por una carrera) intenta desactivarla.
    let fantasma = Actor::Administrador {
        usuario: IdUsuario(99),
    };
    assert!(matches!(
        e.casos
            .cambiar_estado_administrador
            .ejecutar(&fantasma, IdUsuario(1), false)
            .await,
        Err(ErrorIdentidad::UltimoAdministrador)
    ));
    assert!(matches!(
        e.casos
            .cambiar_estado_administrador
            .ejecutar(&LAURA, IdUsuario(2), false)
            .await,
        Err(ErrorIdentidad::AdministradorNoEncontrado)
    ));
}

#[tokio::test]
async fn desactivar_un_administrador_cierra_sus_sesiones() {
    let e = Escenario::nuevo();
    e.casos
        .invitar_administrador
        .ejecutar(&LAURA, "camila@diamante.co", "Camila")
        .await
        .unwrap();
    let token = e.token_del_ultimo_correo();
    e.casos
        .activar_cuenta
        .ejecutar(&token, CLAVE.into())
        .await
        .unwrap();
    let sesion = e.entrar("camila@diamante.co", CLAVE).await.unwrap();
    let camila = e
        .usuarios
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|(u, _)| u.correo.como_str() == "camila@diamante.co")
        .unwrap()
        .0
        .id;

    e.casos
        .cambiar_estado_administrador
        .ejecutar(&LAURA, camila, false)
        .await
        .unwrap();
    assert_eq!(e.actor(&sesion).await, None);
    assert!(matches!(
        e.entrar("camila@diamante.co", CLAVE).await,
        Err(ErrorIdentidad::AccesoSuspendido)
    ));
    assert!(e.auditado("desactivar_administrador"));

    let lista = e.casos.listar_administradores.ejecutar(&LAURA).await.unwrap();
    assert_eq!(lista.len(), 2);
    let barbero = Actor::Barbero {
        usuario: IdUsuario(2),
        barbero: IdBarbero(7),
    };
    assert!(matches!(
        e.casos.listar_administradores.ejecutar(&barbero).await,
        Err(ErrorIdentidad::Prohibido)
    ));
}

// ---------------------------------------------------------------------------------------------
// Fachada de cuentas (la usa el módulo `barberos`)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn crea_la_cuenta_de_un_barbero_y_la_invita_solo_si_tiene_acceso() {
    use crate::api::CuentasPersonal;
    let e = Escenario::nuevo();
    let cuentas = e.casos.cuentas.clone();
    let correo = Correo::nuevo("andres@diamante.co").unwrap();
    let sin_acceso = cuentas
        .crear_cuenta_barbero(&mut (), &LAURA, &correo, "Andrés Mora", false)
        .await
        .unwrap();
    assert!(e.buzon.0.lock().unwrap().is_empty());
    let cuenta = cuentas.cuenta(&mut (), sin_acceso).await.unwrap().unwrap();
    assert_eq!((cuenta.acceso_panel, cuenta.pendiente_activacion), (false, true));

    let repetido = cuentas
        .crear_cuenta_barbero(
            &mut (),
            &LAURA,
            &Correo::nuevo("ANDRES@diamante.co").unwrap(),
            "Otro",
            true,
        )
        .await;
    assert!(matches!(repetido, Err(ErrorIdentidad::CorreoEnUso)));

    // Darle acceso lo invita; con el enlace crea su contraseña y entra.
    cuentas
        .cambiar_acceso(&mut (), &LAURA, sin_acceso, true, "Andrés Mora")
        .await
        .unwrap();
    let token = e.token_del_ultimo_correo();
    e.casos
        .activar_cuenta
        .ejecutar(&token, CLAVE.into())
        .await
        .unwrap();
    e.perfiles.0.lock().unwrap().insert(
        sin_acceso,
        PerfilBarbero {
            barbero: IdBarbero(8),
            nombre: "Andrés Mora".into(),
            puede_acceder: true,
        },
    );
    let sesion = e.entrar("andres@diamante.co", CLAVE).await.unwrap();

    // Quitarle el acceso cierra su sesión y ya no puede entrar.
    cuentas
        .cambiar_acceso(&mut (), &LAURA, sin_acceso, false, "Andrés Mora")
        .await
        .unwrap();
    assert_eq!(e.actor(&sesion).await, None);
    assert!(matches!(
        e.entrar("andres@diamante.co", CLAVE).await,
        Err(ErrorIdentidad::AccesoSuspendido)
    ));
    assert!(e.auditado("crear_cuenta_barbero") && e.auditado("dar_acceso") && e.auditado("quitar_acceso"));
}

#[tokio::test]
async fn cambiar_el_correo_reenvia_la_invitacion_pendiente() {
    use crate::api::CuentasPersonal;
    let e = Escenario::nuevo();
    let cuentas = e.casos.cuentas.clone();
    let id = cuentas
        .crear_cuenta_barbero(
            &mut (),
            &LAURA,
            &Correo::nuevo("andres@diamante.co").unwrap(),
            "Andrés",
            true,
        )
        .await
        .unwrap();
    let nuevo = Correo::nuevo("andres.mora@diamante.co").unwrap();
    cuentas
        .cambiar_correo(&mut (), &LAURA, id, &nuevo, "Andrés")
        .await
        .unwrap();
    let ultimo = e.buzon.0.lock().unwrap().last().unwrap().destinatario.clone();
    assert_eq!(ultimo, nuevo);
    assert!(matches!(
        cuentas
            .cambiar_correo(
                &mut (),
                &LAURA,
                id,
                &Correo::nuevo("laura@diamante.co").unwrap(),
                "Andrés"
            )
            .await,
        Err(ErrorIdentidad::CorreoEnUso)
    ));
    let barbero = Actor::Barbero {
        usuario: IdUsuario(2),
        barbero: IdBarbero(7),
    };
    assert!(matches!(
        cuentas.cambiar_correo(&mut (), &barbero, id, &nuevo, "x").await,
        Err(ErrorIdentidad::Prohibido)
    ));
}

#[tokio::test]
async fn el_enlace_de_acceso_activa_o_restablece_segun_la_cuenta() {
    let e = Escenario::nuevo();
    e.casos
        .invitar_administrador
        .ejecutar(&LAURA, "camila@diamante.co", "Camila Rojas")
        .await
        .unwrap();
    // Sin activar: enlace de activación nuevo, que sirve aunque no se haya enviado ningún correo.
    let enlace = e
        .casos
        .emitir_enlace_de_acceso
        .ejecutar("camila@diamante.co")
        .await
        .unwrap();
    assert!(
        enlace.starts_with("https://diamante.co/acceso/activar#token="),
        "{enlace}"
    );
    let token = enlace.split_once("#token=").unwrap().1;
    e.casos
        .activar_cuenta
        .ejecutar(token, CLAVE.into())
        .await
        .unwrap();
    assert!(e.entrar("camila@diamante.co", CLAVE).await.is_ok());
    // Activada: enlace de restablecimiento.
    let enlace = e
        .casos
        .emitir_enlace_de_acceso
        .ejecutar("camila@diamante.co")
        .await
        .unwrap();
    assert!(
        enlace.starts_with("https://diamante.co/acceso/restablecer#token="),
        "{enlace}"
    );
    assert!(matches!(
        e.casos
            .emitir_enlace_de_acceso
            .ejecutar("nadie@diamante.co")
            .await,
        Err(ErrorIdentidad::CuentaNoEncontrada)
    ));
}
