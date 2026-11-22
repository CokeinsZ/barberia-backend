//! Raíz de composición: construye los adaptadores y los inyecta en los módulos. Es el único
//! lugar que conoce a todos; los módulos nunca crean sus dependencias externas.

use std::sync::Arc;

use agenda::{DependenciasAgenda, ModuloAgenda};
use archivos::{DependenciasArchivos, ModuloArchivos};
use auditoria::ModuloAuditoria;
use barberos::{DependenciasBarberos, ModuloBarberos};
use catalogo::{DependenciasCatalogo, ModuloCatalogo};
use clientes::{DependenciasClientes, ModuloClientes};
use identidad::{DependenciasIdentidad, ModuloIdentidad};
use negocio::ModuloNegocio;
use notificaciones::{DependenciasNotificaciones, ModuloNotificaciones};
use nucleo::eventos::BusEventos;
use nucleo::{Reloj, RelojDePruebas, RelojSistema};
use plataforma::base_datos::{UnidadDeTrabajo, conectar};
use plataforma::configuracion::Configuracion;
use plataforma::http::{ConfianzaProxy, LimitesDeUso, OrigenesPermitidos, Sesiones};
use plataforma::seguridad::{Cifrador, FirmadorUrls};
use reportes::{DependenciasReportes, ModuloReportes};
use sqlx::PgPool;
use trabajos::ModuloTrabajos;
use ventas::{DependenciasVentas, ModuloVentas};
use verificacion::{DependenciasVerificacion, ModuloVerificacion};

use crate::ErrorDeArranque;
use crate::adaptadores::{AgendaParaClientes, ClientesParaVerificacion};
use crate::tareas;

/// Clave AES fija solo para desarrollo y pruebas, cuando no se define `DIAMANTE_CLAVE_CIFRADO`
/// (en producción la configuración la exige). Así los datos cifrados sobreviven a un reinicio.
const CLAVE_CIFRADO_DESARROLLO: [u8; 32] = *b"diamante-solo-desarrollo-000000!";

#[derive(Debug, Clone)]
pub(crate) struct Composicion {
    pub(crate) pool: PgPool,
    pub(crate) reloj: Arc<dyn Reloj>,
    pub(crate) sesiones: Sesiones,
    pub(crate) origenes: OrigenesPermitidos,
    pub(crate) confianza_proxy: ConfianzaProxy,
    /// Límites de uso por IP y por sesión (sin ellos solo en pruebas de carga).
    pub(crate) limites: Option<LimitesDeUso>,
    /// El sitio se sirve por HTTPS: las respuestas llevan `Strict-Transport-Security`.
    pub(crate) https: bool,
    pub(crate) identidad: ModuloIdentidad,
    pub(crate) negocio: ModuloNegocio,
    pub(crate) auditoria: ModuloAuditoria,
    pub(crate) notificaciones: ModuloNotificaciones,
    pub(crate) archivos: ModuloArchivos,
    pub(crate) catalogo: ModuloCatalogo,
    pub(crate) barberos: ModuloBarberos,
    pub(crate) clientes: ModuloClientes,
    pub(crate) verificacion: ModuloVerificacion,
    pub(crate) agenda: ModuloAgenda,
    pub(crate) ventas: ModuloVentas,
    pub(crate) reportes: ModuloReportes,
    pub(crate) trabajos: ModuloTrabajos,
    /// Rutas `/api/v1/_pruebas/*` (solo con `DIAMANTE_PRUEBAS_E2E=true`).
    pub(crate) rutas_de_pruebas: bool,
    /// Reloj ajustable con `/_pruebas/reloj` (solo con las rutas de pruebas).
    pub(crate) reloj_de_pruebas: Option<Arc<RelojDePruebas>>,
}

impl Composicion {
    pub(crate) async fn construir(configuracion: &Configuracion) -> Result<Self, ErrorDeArranque> {
        let pool = conectar(
            &configuracion.base_datos.url,
            configuracion.base_datos.max_conexiones,
        )
        .await?;
        if configuracion.rutas_de_pruebas {
            Self::con_reloj_de_pruebas(pool, Arc::new(RelojDePruebas::default()), configuracion)
        } else {
            Self::con_pool(pool, Arc::new(RelojSistema), configuracion)
        }
    }

    pub(crate) fn con_pool(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        configuracion: &Configuracion,
    ) -> Result<Self, ErrorDeArranque> {
        Self::armar(pool, reloj, None, configuracion)
    }

    /// Con el reloj de pruebas, que `/_pruebas/reloj` puede mover.
    pub(crate) fn con_reloj_de_pruebas(
        pool: PgPool,
        reloj: Arc<RelojDePruebas>,
        configuracion: &Configuracion,
    ) -> Result<Self, ErrorDeArranque> {
        Self::armar(pool, reloj.clone(), Some(reloj), configuracion)
    }

    /// Orden de construcción: primero el bus y los módulos que aportan puertos (auditoría,
    /// notificaciones, archivos, parámetros del negocio y los adaptadores de `barberos` que
    /// necesitan `identidad` y `catalogo`), después los que los usan y, al final, las
    /// suscripciones a eventos y la cola de trabajos.
    fn armar(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        reloj_de_pruebas: Option<Arc<RelojDePruebas>>,
        configuracion: &Configuracion,
    ) -> Result<Self, ErrorDeArranque> {
        // Bus de eventos en la transacción: cada módulo suscribe aquí sus reacciones.
        let bus: Arc<BusEventos<UnidadDeTrabajo>> = Arc::new(BusEventos::nuevo());
        let auditoria = ModuloAuditoria::con_postgres(pool.clone(), reloj.clone());
        let notificaciones = ModuloNotificaciones::con_postgres(DependenciasNotificaciones {
            pool: pool.clone(),
            reloj: reloj.clone(),
            smtp: configuracion.smtp.as_ref(),
            whatsapp: &configuracion.whatsapp,
            bus: bus.clone(),
        })?;
        let firmador = match &configuracion.clave_firma {
            Some(clave) => FirmadorUrls::nuevo(clave.exponer().as_bytes()),
            None => {
                tracing::warn!("sin DIAMANTE_CLAVE_FIRMA: las URL firmadas dejan de servir al reiniciar");
                FirmadorUrls::aleatorio()?
            }
        };
        let cifrador = match &configuracion.clave_cifrado {
            Some(clave) => Cifrador::nuevo(&clave.0),
            None => {
                tracing::warn!("sin DIAMANTE_CLAVE_CIFRADO: se usa la clave fija de desarrollo");
                Cifrador::nuevo(&CLAVE_CIFRADO_DESARROLLO)
            }
        };
        let firmador_codigos = firmador.clone();
        let archivos = ModuloArchivos::con_postgres(DependenciasArchivos {
            pool: pool.clone(),
            reloj: reloj.clone(),
            raiz: configuracion.archivos_dir.clone(),
            firmador,
        });
        let negocio = ModuloNegocio::con_postgres(pool.clone(), reloj.clone(), auditoria.auditor());
        let identidad = ModuloIdentidad::con_postgres(DependenciasIdentidad {
            pool: pool.clone(),
            reloj: reloj.clone(),
            perfiles: ModuloBarberos::perfiles_para_identidad(reloj.clone()),
            auditor: auditoria.auditor(),
            buzon: notificaciones.buzon(),
            url_publica: configuracion.url_publica.clone(),
        });
        let catalogo = ModuloCatalogo::con_postgres(DependenciasCatalogo {
            pool: pool.clone(),
            auditor: auditoria.auditor(),
            oferentes: ModuloBarberos::oferentes_para_catalogo(reloj.clone()),
        });
        let barberos = ModuloBarberos::con_postgres(DependenciasBarberos {
            pool: pool.clone(),
            reloj: reloj.clone(),
            cuentas: identidad.cuentas(),
            catalogo: catalogo.servicios(),
            archivos: archivos.archivos(),
            parametros: negocio.parametros(),
            auditor: auditoria.auditor(),
            bus: bus.clone(),
            cifrador,
        });
        // `clientes` pregunta a la agenda (construida después) si un cliente tiene citas por
        // atender: el adaptador se conecta cuando la agenda existe.
        let agenda_para_clientes = Arc::new(AgendaParaClientes::default());
        let clientes = ModuloClientes::con_postgres(DependenciasClientes {
            pool: pool.clone(),
            reloj: reloj.clone(),
            auditor: auditoria.auditor(),
            agenda: agenda_para_clientes.clone(),
        });
        let verificacion = ModuloVerificacion::con_postgres(DependenciasVerificacion {
            pool: pool.clone(),
            reloj: reloj.clone(),
            buzon: notificaciones.buzon(),
            clientes: Arc::new(ClientesParaVerificacion(clientes.clientes())),
            firmador: firmador_codigos,
        });
        let agenda = ModuloAgenda::con_postgres(DependenciasAgenda {
            pool: pool.clone(),
            reloj: reloj.clone(),
            equipo: barberos.equipo(),
            catalogo: catalogo.servicios(),
            calendario: negocio.calendario(),
            negocio: negocio.datos_publicos(),
            clientes: clientes.clientes(),
            comprobantes: verificacion.comprobantes(),
            buzon: notificaciones.buzon(),
            avisos: notificaciones.avisos(),
            mensajes: notificaciones.mensajes(),
            auditor: auditoria.auditor(),
            bus: bus.clone(),
            url_publica: configuracion.url_publica.clone(),
        });

        agenda_para_clientes.conectar(agenda.citas_de_clientes());
        let ventas = ModuloVentas::con_postgres(DependenciasVentas {
            pool: pool.clone(),
            reloj: reloj.clone(),
            citas: agenda.citas_para_ventas(),
            equipo: barberos.equipo_para_ventas(),
            catalogo: catalogo.servicios(),
            clientes: clientes.clientes(),
            auditor: auditoria.auditor(),
            bus: bus.clone(),
        });

        // Solo lectura de las vistas publicadas; la única escritura es la auditoría de los PDF.
        if configuracion.gotenberg_url.is_none() {
            tracing::warn!("sin DIAMANTE_GOTENBERG_URL: los PDF de los reportes responden 503");
        }
        let reportes = ModuloReportes::con_postgres(DependenciasReportes {
            pool: pool.clone(),
            reloj: reloj.clone(),
            equipo: barberos.equipo(),
            calendario: negocio.calendario(),
            negocio: negocio.parametros(),
            clientes: clientes.clientes(),
            avisos: notificaciones.avisos(),
            auditor: auditoria.auditor(),
            gotenberg_url: configuracion.gotenberg_url.clone(),
        });

        // Suscripciones (se agregan con el bus ya repartido: los manejadores usan fachadas
        // construidas arriba).
        ModuloClientes::suscribir(&bus);
        agenda.suscribir(&bus);

        let trabajos = ModuloTrabajos::con_postgres(
            pool.clone(),
            reloj.clone(),
            tareas::programaciones(),
            tareas::tareas(&tareas::Modulos {
                pool: pool.clone(),
                reloj: reloj.clone(),
                identidad: identidad.clone(),
                verificacion: verificacion.clone(),
                agenda: agenda.clone(),
                notificaciones: notificaciones.clone(),
            }),
            tareas::nombre_del_proceso(),
        );

        Ok(Self {
            pool,
            reloj,
            sesiones: Sesiones(identidad.validador_sesiones()),
            origenes: OrigenesPermitidos(Arc::new(configuracion.origenes_permitidos.clone())),
            confianza_proxy: ConfianzaProxy(configuracion.confiar_proxy),
            limites: configuracion.limites_de_uso.then(LimitesDeUso::default),
            https: configuracion.url_publica.starts_with("https://"),
            identidad,
            negocio,
            auditoria,
            notificaciones,
            archivos,
            catalogo,
            barberos,
            clientes,
            verificacion,
            agenda,
            ventas,
            reportes,
            trabajos,
            rutas_de_pruebas: configuracion.rutas_de_pruebas,
            reloj_de_pruebas,
        })
    }
}
