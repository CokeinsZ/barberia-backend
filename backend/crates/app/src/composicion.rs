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
use notificaciones::ModuloNotificaciones;
use nucleo::eventos::BusEventos;
use nucleo::{Reloj, RelojSistema};
use plataforma::base_datos::{UnidadDeTrabajo, conectar};
use plataforma::configuracion::Configuracion;
use plataforma::http::{ConfianzaProxy, OrigenesPermitidos, Sesiones};
use plataforma::seguridad::{Cifrador, FirmadorUrls};
use sqlx::PgPool;
use verificacion::{DependenciasVerificacion, ModuloVerificacion};

use crate::ErrorDeArranque;
use crate::adaptadores::ClientesParaVerificacion;

/// Clave AES fija solo para desarrollo y pruebas, cuando no se define `DIAMANTE_CLAVE_CIFRADO`
/// (en producción la configuración la exige). Así los datos cifrados sobreviven a un reinicio.
const CLAVE_CIFRADO_DESARROLLO: [u8; 32] = *b"diamante-solo-desarrollo-000000!";

#[derive(Debug, Clone)]
pub(crate) struct Composicion {
    pub(crate) pool: PgPool,
    pub(crate) sesiones: Sesiones,
    pub(crate) origenes: OrigenesPermitidos,
    pub(crate) confianza_proxy: ConfianzaProxy,
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
    /// `GET /api/v1/_pruebas/mensajes` (solo con `DIAMANTE_PRUEBAS_E2E=true`).
    pub(crate) rutas_de_pruebas: bool,
}

impl Composicion {
    pub(crate) async fn construir(configuracion: &Configuracion) -> Result<Self, ErrorDeArranque> {
        let pool = conectar(
            &configuracion.base_datos.url,
            configuracion.base_datos.max_conexiones,
        )
        .await?;
        Self::con_pool(pool, Arc::new(RelojSistema), configuracion)
    }

    /// Orden de construcción: primero los módulos que aportan puertos (auditoría, buzón de
    /// salida, archivos, parámetros del negocio y los adaptadores de `barberos` que necesitan
    /// `identidad` y `catalogo`) y después los que los usan.
    pub(crate) fn con_pool(
        pool: PgPool,
        reloj: Arc<dyn Reloj>,
        configuracion: &Configuracion,
    ) -> Result<Self, ErrorDeArranque> {
        let auditoria = ModuloAuditoria::con_postgres(pool.clone(), reloj.clone());
        let notificaciones =
            ModuloNotificaciones::con_postgres(pool.clone(), reloj.clone(), configuracion.smtp.as_ref())?;
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
        // Bus de eventos en la transacción: cada módulo suscribe aquí sus reacciones.
        let mut bus: BusEventos<UnidadDeTrabajo> = BusEventos::nuevo();
        ModuloClientes::suscribir(&mut bus);
        let bus = Arc::new(bus);
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
        let clientes = ModuloClientes::con_postgres(DependenciasClientes {
            pool: pool.clone(),
            reloj: reloj.clone(),
            auditor: auditoria.auditor(),
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
            reloj,
            equipo: barberos.equipo(),
            catalogo: catalogo.servicios(),
            calendario: negocio.calendario(),
            negocio: negocio.datos_publicos(),
            clientes: clientes.clientes(),
            comprobantes: verificacion.comprobantes(),
            buzon: notificaciones.buzon(),
            auditor: auditoria.auditor(),
            bus,
            url_publica: configuracion.url_publica.clone(),
        });

        Ok(Self {
            pool,
            sesiones: Sesiones(identidad.validador_sesiones()),
            origenes: OrigenesPermitidos(Arc::new(configuracion.origenes_permitidos.clone())),
            confianza_proxy: ConfianzaProxy(configuracion.confiar_proxy),
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
            rutas_de_pruebas: configuracion.rutas_de_pruebas,
        })
    }
}
