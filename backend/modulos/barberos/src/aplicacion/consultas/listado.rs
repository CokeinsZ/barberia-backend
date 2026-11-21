use nucleo::Actor;
use nucleo::identificadores::IdBarbero;

use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::{BarberoDto, IndicadoresEquipoDto, ListadoBarberosDto};
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::{barbero_propio, solo_administrador};
use crate::dominio::EstadoBarbero;

caso_de_uso!(
    /// Listado del equipo con su estado de hoy, búsqueda y filtro por estado, e indicadores
    /// (RF-BAR-01, RF-BAR-02). Solo administradores.
    ListarBarberos
);

impl<C: Send> ListarBarberos<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        busqueda: Option<String>,
        estado: Option<String>,
    ) -> Result<ListadoBarberosDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let filtro_estado = match estado.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
            None => None,
            Some(codigo) => Some(EstadoBarbero::desde_codigo(codigo).ok_or_else(|| {
                ErrorBarberos::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
                    "estado",
                    "Elige activo, vacaciones, incapacidad_licencia, suspendido o inactivo.",
                )])
            })?),
        };
        let busqueda = busqueda.map(|q| normalizar(&q)).filter(|q| !q.is_empty());

        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let todos = p.barberos.listar(&mut cx, p.hoy()).await?;
        let meta = IndicadoresEquipoDto {
            registrados: todos.len(),
            activos: todos.iter().filter(|b| b.estado == EstadoBarbero::Activo).count(),
            ausencias_temporales: todos.iter().filter(|b| b.estado.es_ausencia()).count(),
            inactivos: todos
                .iter()
                .filter(|b| b.estado == EstadoBarbero::Inactivo)
                .count(),
        };
        let mut datos = Vec::new();
        for barbero in todos {
            if filtro_estado.is_some_and(|e| e != barbero.estado) {
                continue;
            }
            if let Some(q) = &busqueda {
                let coincide = normalizar(&barbero.nombre()).contains(q.as_str())
                    || barbero.numero_documento.to_lowercase().contains(q.as_str())
                    || barbero.celular.contains(q.as_str());
                if !coincide {
                    continue;
                }
            }
            datos.push(p.dto(&mut cx, barbero, false).await?);
        }
        p.transacciones.confirmar(cx).await?;
        Ok(ListadoBarberosDto { datos, meta })
    }
}

/// Minúsculas, sin tildes y sin espacios repetidos, para buscar.
fn normalizar(texto: &str) -> String {
    texto
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            otro => otro,
        })
        .collect()
}

caso_de_uso!(
    /// Ficha completa del barbero (RF-BAR-04) para el administrador, y «Mi perfil» en solo
    /// lectura para el propio barbero (`GET /yo`, RF-PBA-07).
    ObtenerFicha
);

impl<C: Send> ObtenerFicha<C> {
    pub(crate) async fn ejecutar(&self, actor: &Actor, id: IdBarbero) -> Result<BarberoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        self.ficha(id).await
    }

    pub(crate) async fn yo(&self, actor: &Actor) -> Result<BarberoDto, ErrorBarberos> {
        let id = barbero_propio(actor)?;
        self.ficha(id).await
    }

    async fn ficha(&self, id: IdBarbero) -> Result<BarberoDto, ErrorBarberos> {
        let mut cx = self.p.transacciones.iniciar().await?;
        let ficha = self.p.ficha(&mut cx, id).await?;
        self.p.transacciones.confirmar(cx).await?;
        Ok(ficha)
    }
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn la_busqueda_ignora_tildes_y_mayusculas() {
        assert_eq!(super::normalizar("  Andrés   GÓMEZ "), "andres gomez");
    }
}
