//! `diamante migrar`: reúne las migraciones de la base compartida y de cada módulo y las
//! aplica en orden global de versión.

use plataforma::base_datos::{
    MIGRACIONES_BASE, OrigenMigraciones, aplicar_migraciones, combinar_migraciones, conectar,
};
use plataforma::configuracion::Configuracion;

use crate::ErrorDeArranque;

/// Todas las migraciones del sistema. Al crear un módulo con esquema propio, se agrega aquí.
pub(crate) fn origenes() -> Vec<OrigenMigraciones> {
    vec![
        OrigenMigraciones {
            modulo: "base",
            migraciones: &MIGRACIONES_BASE,
        },
        OrigenMigraciones {
            modulo: "identidad",
            migraciones: &identidad::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "archivos",
            migraciones: &archivos::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "negocio",
            migraciones: &negocio::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "catalogo",
            migraciones: &catalogo::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "barberos",
            migraciones: &barberos::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "clientes",
            migraciones: &clientes::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "verificacion",
            migraciones: &verificacion::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "notificaciones",
            migraciones: &notificaciones::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "agenda",
            migraciones: &agenda::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "ventas",
            migraciones: &ventas::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "auditoria",
            migraciones: &auditoria::MIGRACIONES,
        },
        OrigenMigraciones {
            modulo: "trabajos",
            migraciones: &trabajos::MIGRACIONES,
        },
    ]
}

pub(crate) async fn ejecutar(configuracion: &Configuracion) -> Result<(), ErrorDeArranque> {
    let migrador = combinar_migraciones(&origenes())?;
    let pool = conectar(&configuracion.base_datos.url_migraciones, 1).await?;
    aplicar_migraciones(&pool, &migrador).await?;
    pool.close().await;
    tracing::info!(total = migrador.iter().count(), "migraciones al día");
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_migraciones_se_combinan_en_el_orden_de_dependencias() {
        let migrador = combinar_migraciones(&origenes()).unwrap();
        let descripciones: Vec<_> = migrador.iter().map(|m| m.description.to_string()).collect();
        let modulos: Vec<_> = descripciones
            .iter()
            .map(|d| d.split("  ").next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            modulos,
            [
                "base",
                "identidad",
                "archivos",
                "negocio",
                "catalogo",
                "barberos",
                "clientes",
                "verificacion",
                "notificaciones",
                "agenda",
                "ventas",
                "auditoria",
                "trabajos",
                // Fase 4: propósitos nuevos de los mensajes de una cita (va al final porque
                // las migraciones se ordenan por versión).
                "agenda",
                // Fase 6: vistas publicadas para `reportes` (líneas y pagos de las ventas).
                "ventas"
            ]
        );
    }
}
