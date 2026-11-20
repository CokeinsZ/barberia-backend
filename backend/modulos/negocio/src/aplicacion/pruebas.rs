//! Pruebas de los casos de uso con repositorios en memoria (sin base de datos).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use nucleo::identificadores::{IdBarbero, IdDiaEspecial, IdUsuario};
use nucleo::{Actor, ErrorPersistencia, RelojControlado};

use super::CasosDeUso;
use super::comandos::NuevoDiaEspecial;
use super::errores::ErrorNegocio;
use super::puertos::{
    DiaEspecialGuardado, RESTRICCION_FECHA_UNICA, RepositorioDiasEspeciales, RepositorioNegocio,
};
use crate::dominio::entidades::{DatosNegocio, DiaEspecial, PropuestaDatosNegocio};

#[derive(Debug, Default)]
struct NegocioEnMemoria(Mutex<Option<DatosNegocio>>);

#[async_trait]
impl RepositorioNegocio for NegocioEnMemoria {
    async fn obtener(&self) -> Result<DatosNegocio, ErrorPersistencia> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ErrorPersistencia::Otro("sin datos".into()))
    }
    async fn guardar(&self, datos: &DatosNegocio) -> Result<(), ErrorPersistencia> {
        *self.0.lock().unwrap() = Some(datos.clone());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct DiasEnMemoria {
    dias: Mutex<Vec<DiaEspecialGuardado>>,
    siguiente: AtomicI64,
}

#[async_trait]
impl RepositorioDiasEspeciales for DiasEnMemoria {
    async fn listar(
        &self,
        desde: NaiveDate,
        hasta: NaiveDate,
    ) -> Result<Vec<DiaEspecialGuardado>, ErrorPersistencia> {
        let mut dias: Vec<_> = self
            .dias
            .lock()
            .unwrap()
            .iter()
            .filter(|g| (desde..=hasta).contains(&g.dia.fecha()))
            .cloned()
            .collect();
        dias.sort_by_key(|g| g.dia.fecha());
        Ok(dias)
    }
    async fn registrar(&self, dia: &DiaEspecial) -> Result<IdDiaEspecial, ErrorPersistencia> {
        let mut dias = self.dias.lock().unwrap();
        if dias.iter().any(|g| g.dia.fecha() == dia.fecha()) {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: RESTRICCION_FECHA_UNICA.into(),
            });
        }
        let id = IdDiaEspecial(self.siguiente.fetch_add(1, Ordering::Relaxed) + 1);
        dias.push(DiaEspecialGuardado { id, dia: dia.clone() });
        Ok(id)
    }
    async fn eliminar(&self, id: IdDiaEspecial) -> Result<bool, ErrorPersistencia> {
        let mut dias = self.dias.lock().unwrap();
        let antes = dias.len();
        dias.retain(|g| g.id != id);
        Ok(dias.len() < antes)
    }
}

/// Miércoles 18 de noviembre de 2026, 2:50 p. m. en Bogotá.
fn casos() -> CasosDeUso {
    let reloj = Arc::new(RelojControlado::en("2026-11-18T19:50:00Z".parse().unwrap()));
    CasosDeUso::nuevos(
        Arc::new(NegocioEnMemoria::default()),
        Arc::new(DiasEnMemoria::default()),
        reloj,
    )
}

fn admin() -> Actor {
    Actor::Administrador {
        usuario: IdUsuario(1),
    }
}

fn barbero() -> Actor {
    Actor::Barbero {
        usuario: IdUsuario(2),
        barbero: IdBarbero(1),
    }
}

fn fecha(texto: &str) -> NaiveDate {
    texto.parse().unwrap()
}

fn propuesta() -> PropuestaDatosNegocio {
    PropuestaDatosNegocio {
        nombre_comercial: "Diamante Barbería".into(),
        whatsapp: Some("300 555 0000".into()),
        porcentaje_comision_defecto: 50.0,
        ..Default::default()
    }
}

fn nochebuena() -> NuevoDiaEspecial {
    NuevoDiaEspecial {
        fecha: fecha("2026-12-24"),
        nombre: "Nochebuena".into(),
        tipo: "cierre_temprano".into(),
        hora_cierre: "14:00:00".parse().ok(),
    }
}

fn campos(error: ErrorNegocio) -> Vec<String> {
    match error {
        ErrorNegocio::DatosInvalidos(errores) => errores.into_iter().map(|e| e.campo).collect(),
        otro => panic!("se esperaba DatosInvalidos y llegó {otro:?}"),
    }
}

#[tokio::test]
async fn el_administrador_actualiza_y_consulta_los_datos() {
    let casos = casos();
    let dto = casos
        .actualizar_datos
        .ejecutar(&admin(), propuesta())
        .await
        .unwrap();
    assert_eq!(dto.whatsapp.as_deref(), Some("3005550000"));
    assert_eq!(casos.obtener_datos.ejecutar(&admin()).await.unwrap(), dto);
}

#[tokio::test]
async fn el_barbero_no_puede_ver_ni_cambiar_los_datos_del_negocio() {
    let casos = casos();
    assert!(matches!(
        casos.actualizar_datos.ejecutar(&barbero(), propuesta()).await,
        Err(ErrorNegocio::Prohibido)
    ));
    assert!(matches!(
        casos.obtener_datos.ejecutar(&barbero()).await,
        Err(ErrorNegocio::Prohibido)
    ));
}

#[tokio::test]
async fn los_datos_invalidos_no_se_guardan() {
    let casos = casos();
    let invalida = PropuestaDatosNegocio {
        porcentaje_comision_defecto: -5.0,
        ..propuesta()
    };
    let error = casos
        .actualizar_datos
        .ejecutar(&admin(), invalida)
        .await
        .unwrap_err();
    assert_eq!(campos(error), ["porcentaje_comision_defecto"]);
    assert!(casos.obtener_datos.ejecutar(&admin()).await.is_err());
}

#[tokio::test]
async fn registra_lista_y_elimina_dias_especiales() {
    let casos = casos();
    let creado = casos
        .registrar_dia
        .ejecutar(&admin(), nochebuena())
        .await
        .unwrap();
    assert_eq!(creado.hora_cierre, "14:00:00".parse().ok());

    let lista = casos.listar_dias.ejecutar(&barbero(), None, None).await.unwrap();
    assert_eq!(lista, std::slice::from_ref(&creado));

    casos.eliminar_dia.ejecutar(&admin(), creado.id).await.unwrap();
    assert!(matches!(
        casos.eliminar_dia.ejecutar(&admin(), creado.id).await,
        Err(ErrorNegocio::DiaEspecialNoEncontrado)
    ));
}

#[tokio::test]
async fn una_fecha_solo_admite_un_dia_especial() {
    let casos = casos();
    casos
        .registrar_dia
        .ejecutar(&admin(), nochebuena())
        .await
        .unwrap();
    let otro = NuevoDiaEspecial {
        tipo: "cerrado".into(),
        hora_cierre: None,
        ..nochebuena()
    };
    let error = casos.registrar_dia.ejecutar(&admin(), otro).await.unwrap_err();
    assert!(matches!(error, ErrorNegocio::FechaOcupada(f) if f == fecha("2026-12-24")));
    assert_eq!(
        error.to_string(),
        "Ya hay un día especial registrado el 24/12/2026."
    );
}

#[tokio::test]
async fn no_registra_fechas_pasadas_segun_el_reloj() {
    let casos = casos();
    let ayer = NuevoDiaEspecial {
        fecha: fecha("2026-11-17"),
        ..nochebuena()
    };
    assert_eq!(
        campos(casos.registrar_dia.ejecutar(&admin(), ayer).await.unwrap_err()),
        ["fecha"]
    );
}

#[tokio::test]
async fn valida_el_rango_de_consulta() {
    let casos = casos();
    let admin = admin();
    let invertido = casos
        .listar_dias
        .ejecutar(&admin, Some(fecha("2026-12-31")), Some(fecha("2026-12-01")));
    assert_eq!(campos(invertido.await.unwrap_err()), ["hasta"]);
    let enorme = casos
        .listar_dias
        .ejecutar(&admin, Some(fecha("2026-01-01")), Some(fecha("2028-01-01")));
    assert_eq!(campos(enorme.await.unwrap_err()), ["hasta"]);
}

#[tokio::test]
async fn el_cliente_no_consulta_el_calendario_interno() {
    let casos = casos();
    let cliente = Actor::Cliente { cliente: None };
    assert!(matches!(
        casos.listar_dias.ejecutar(&cliente, None, None).await,
        Err(ErrorNegocio::Prohibido)
    ));
}
