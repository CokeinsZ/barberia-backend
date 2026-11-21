//! Pruebas de los casos de uso con repositorios en memoria (sin base de datos).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use auditoria::api::{Auditor, EntradaAuditoria};
use chrono::{NaiveDate, NaiveTime};
use nucleo::identificadores::{IdBarbero, IdDiaEspecial, IdUsuario};
use nucleo::{Actor, ErrorPersistencia, RelojControlado, SinTransaccion};

use super::CasosDeUso;
use super::comandos::NuevoDiaEspecial;
use super::comandos::SolicitudPolitica;
use super::errores::ErrorNegocio;
use super::puertos::{
    ConsultaHorarioAtencion, DiaEspecialGuardado, PoliticaGuardada, Puertos, RESTRICCION_FECHA_UNICA,
    RepositorioDiasEspeciales, RepositorioNegocio, RepositorioPoliticas,
};
use crate::api::ParametrosNegocio;
use crate::dominio::entidades::{DatosNegocio, DiaEspecial, NuevaPolitica, PropuestaDatosNegocio};

#[derive(Debug, Default)]
struct NegocioEnMemoria(Mutex<Option<DatosNegocio>>);

#[async_trait]
impl RepositorioNegocio<()> for NegocioEnMemoria {
    async fn obtener(&self, _: &mut ()) -> Result<DatosNegocio, ErrorPersistencia> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ErrorPersistencia::Otro("sin datos".into()))
    }
    async fn guardar(&self, _: &mut (), datos: &DatosNegocio) -> Result<(), ErrorPersistencia> {
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
impl RepositorioDiasEspeciales<()> for DiasEnMemoria {
    async fn listar(
        &self,
        _: &mut (),
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
    async fn registrar(&self, _: &mut (), dia: &DiaEspecial) -> Result<IdDiaEspecial, ErrorPersistencia> {
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
    async fn eliminar(
        &self,
        _: &mut (),
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia> {
        let mut dias = self.dias.lock().unwrap();
        let posicion = dias.iter().position(|g| g.id == id);
        Ok(posicion.map(|i| dias.remove(i)))
    }
    async fn obtener(
        &self,
        _: &mut (),
        id: IdDiaEspecial,
    ) -> Result<Option<DiaEspecialGuardado>, ErrorPersistencia> {
        Ok(self.dias.lock().unwrap().iter().find(|g| g.id == id).cloned())
    }
    async fn actualizar(
        &self,
        _: &mut (),
        id: IdDiaEspecial,
        dia: &DiaEspecial,
    ) -> Result<(), ErrorPersistencia> {
        let mut dias = self.dias.lock().unwrap();
        if dias.iter().any(|g| g.id != id && g.dia.fecha() == dia.fecha()) {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: RESTRICCION_FECHA_UNICA.into(),
            });
        }
        if let Some(g) = dias.iter_mut().find(|g| g.id == id) {
            g.dia = dia.clone();
        }
        Ok(())
    }
    async fn registrar_si_libre(
        &self,
        cx: &mut (),
        dia: &DiaEspecial,
    ) -> Result<Option<IdDiaEspecial>, ErrorPersistencia> {
        match self.registrar(cx, dia).await {
            Ok(id) => Ok(Some(id)),
            Err(ErrorPersistencia::Duplicado { .. }) => Ok(None),
            Err(otro) => Err(otro),
        }
    }
}

/// Lunes a sábado de 9:00 a 19:00; el domingo nadie trabaja.
#[derive(Debug, Default)]
struct HorarioFijo;

#[async_trait]
impl ConsultaHorarioAtencion<()> for HorarioFijo {
    async fn por_dia(&self, _: &mut ()) -> Result<Vec<(i16, NaiveTime, NaiveTime)>, ErrorPersistencia> {
        let (abre, cierra) = ("09:00:00".parse().unwrap(), "19:00:00".parse().unwrap());
        Ok((1..=6).map(|d| (d, abre, cierra)).collect())
    }
}

#[derive(Debug, Default)]
struct PoliticasEnMemoria(Mutex<Vec<PoliticaGuardada>>);

#[async_trait]
impl RepositorioPoliticas<()> for PoliticasEnMemoria {
    async fn listar(&self, _: &mut ()) -> Result<Vec<PoliticaGuardada>, ErrorPersistencia> {
        let mut todas = self.0.lock().unwrap().clone();
        todas.sort_by_key(|p| std::cmp::Reverse(p.vigente_desde));
        Ok(todas)
    }
    async fn publicar(&self, _: &mut (), p: &NuevaPolitica) -> Result<i64, ErrorPersistencia> {
        let mut todas = self.0.lock().unwrap();
        if todas.iter().any(|g| g.tipo == p.tipo && g.version == p.version) {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: "politica_version_uk".into(),
            });
        }
        let id = i64::try_from(todas.len()).unwrap() + 1;
        todas.push(PoliticaGuardada {
            id,
            tipo: p.tipo,
            version: p.version.clone(),
            url: p.url.clone(),
            vigente_desde: p.vigente_desde,
        });
        Ok(id)
    }
}

/// Acciones auditadas, en orden.
#[derive(Debug, Default)]
struct AuditoriaEnMemoria(Mutex<Vec<&'static str>>);

#[async_trait]
impl Auditor<()> for AuditoriaEnMemoria {
    async fn registrar(&self, _: &mut (), entrada: EntradaAuditoria) -> Result<(), ErrorPersistencia> {
        self.0.lock().unwrap().push(entrada.accion);
        Ok(())
    }
}

/// Miércoles 18 de noviembre de 2026, 2:50 p. m. en Bogotá.
fn casos() -> CasosDeUso<()> {
    casos_con_auditoria().0
}

fn casos_con_auditoria() -> (CasosDeUso<()>, Arc<AuditoriaEnMemoria>) {
    let reloj = Arc::new(RelojControlado::en("2026-11-18T19:50:00Z".parse().unwrap()));
    let auditoria = Arc::new(AuditoriaEnMemoria::default());
    let casos = CasosDeUso::nuevos(Puertos {
        transacciones: Arc::new(SinTransaccion),
        negocio: Arc::new(NegocioEnMemoria::default()),
        dias: Arc::new(DiasEnMemoria::default()),
        horario: Arc::new(HorarioFijo),
        politicas: Arc::new(PoliticasEnMemoria::default()),
        auditor: auditoria.clone(),
        reloj,
    });
    (casos, auditoria)
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
    let (casos, auditoria) = casos_con_auditoria();
    let dto = casos
        .actualizar_datos
        .ejecutar(&admin(), propuesta())
        .await
        .unwrap();
    assert_eq!(dto.whatsapp.as_deref(), Some("3005550000"));
    assert_eq!(casos.obtener_datos.ejecutar(&admin()).await.unwrap(), dto);
    assert_eq!(*auditoria.0.lock().unwrap(), ["actualizar_datos"]);
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
    let (casos, auditoria) = casos_con_auditoria();
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
    assert_eq!(
        *auditoria.0.lock().unwrap(),
        ["registrar_dia_especial", "eliminar_dia_especial"]
    );
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

#[tokio::test]
async fn edita_un_dia_especial_y_respeta_la_fecha_unica() {
    let (casos, auditoria) = casos_con_auditoria();
    let creado = casos
        .registrar_dia
        .ejecutar(&admin(), nochebuena())
        .await
        .unwrap();
    let otro = NuevoDiaEspecial {
        fecha: fecha("2026-12-31"),
        nombre: "Fin de año".into(),
        tipo: "cerrado".into(),
        hora_cierre: None,
    };
    casos.registrar_dia.ejecutar(&admin(), otro).await.unwrap();

    let cambios = NuevoDiaEspecial {
        hora_cierre: "13:00:00".parse().ok(),
        ..nochebuena()
    };
    let editado = casos
        .editar_dia
        .ejecutar(&admin(), creado.id, cambios)
        .await
        .unwrap();
    assert_eq!(editado.hora_cierre, "13:00:00".parse().ok());

    let a_fin_de_anio = NuevoDiaEspecial {
        fecha: fecha("2026-12-31"),
        ..nochebuena()
    };
    assert!(matches!(
        casos
            .editar_dia
            .ejecutar(&admin(), creado.id, a_fin_de_anio)
            .await,
        Err(ErrorNegocio::FechaOcupada(_))
    ));
    assert!(matches!(
        casos
            .editar_dia
            .ejecutar(&admin(), IdDiaEspecial(99), nochebuena())
            .await,
        Err(ErrorNegocio::DiaEspecialNoEncontrado)
    ));
    assert!(auditoria.0.lock().unwrap().contains(&"editar_dia_especial"));
}

#[tokio::test]
async fn precarga_los_festivos_restantes_del_anio_sin_duplicar() {
    let casos = casos();
    // Hoy es 18/11/2026: de 2026 solo quedan el 8 y el 25 de diciembre.
    let inventario = NuevoDiaEspecial {
        fecha: fecha("2026-12-08"),
        nombre: "Cerrado por inventario".into(),
        tipo: "cerrado".into(),
        hora_cierre: None,
    };
    casos.registrar_dia.ejecutar(&admin(), inventario).await.unwrap();
    let resultado = casos.precargar_festivos.ejecutar(&admin(), 2026).await.unwrap();
    let fechas: Vec<String> = resultado.creados.iter().map(|d| d.fecha.to_string()).collect();
    assert_eq!(fechas, ["2026-12-25"]);
    assert_eq!(resultado.omitidos, 17);
    assert!(resultado.creados[0].automatico);

    let otra_vez = casos.precargar_festivos.ejecutar(&admin(), 2026).await.unwrap();
    assert!(otra_vez.creados.is_empty(), "es idempotente");
    assert_eq!(
        casos
            .precargar_festivos
            .ejecutar(&admin(), 2027)
            .await
            .unwrap()
            .creados
            .len(),
        18
    );
    assert_eq!(
        campos(
            casos
                .precargar_festivos
                .ejecutar(&admin(), 2040)
                .await
                .unwrap_err()
        ),
        ["anio"]
    );
    assert!(matches!(
        casos.precargar_festivos.ejecutar(&barbero(), 2027).await,
        Err(ErrorNegocio::Prohibido)
    ));
}

#[tokio::test]
async fn el_horario_de_atencion_marca_los_dias_sin_barberos() {
    let casos = casos();
    let horario = casos.horario_atencion.ejecutar(&barbero()).await.unwrap();
    assert_eq!(horario.len(), 7);
    assert_eq!((horario[0].nombre, horario[0].abierto), ("Lunes", true));
    assert_eq!(horario[0].abre, "09:00:00".parse().ok());
    assert_eq!(
        (horario[6].nombre, horario[6].abierto, horario[6].abre),
        ("Domingo", false, None)
    );
}

#[tokio::test]
async fn publica_versiones_y_marca_la_vigente_de_cada_tipo() {
    let casos = casos();
    let solicitud = |version: &str, desde: &str| SolicitudPolitica {
        tipo: "tratamiento_datos".into(),
        version: version.into(),
        url: format!("/legal/datos-{version}"),
        vigente_desde: desde.parse().ok(),
    };
    casos
        .publicar_politica
        .ejecutar(&admin(), solicitud("1", "2026-01-01T00:00:00Z"))
        .await
        .unwrap();
    casos
        .publicar_politica
        .ejecutar(&admin(), solicitud("2", "2026-06-01T00:00:00Z"))
        .await
        .unwrap();
    // Programada para el futuro: todavía no rige.
    casos
        .publicar_politica
        .ejecutar(&admin(), solicitud("3", "2027-01-01T00:00:00Z"))
        .await
        .unwrap();
    let repetida = casos
        .publicar_politica
        .ejecutar(&admin(), solicitud("2", "2026-07-01T00:00:00Z"))
        .await;
    assert_eq!(campos(repetida.unwrap_err()), ["version"]);

    let lista = casos.listar_politicas.ejecutar(&admin()).await.unwrap();
    let vigentes: Vec<&str> = lista
        .iter()
        .filter(|p| p.vigente)
        .map(|p| p.version.as_str())
        .collect();
    assert_eq!(vigentes, ["2"]);
}

#[tokio::test]
async fn la_fachada_da_la_comision_por_defecto() {
    let casos = casos();
    let propuesta = PropuestaDatosNegocio {
        porcentaje_comision_defecto: 45.5,
        ..propuesta()
    };
    casos
        .actualizar_datos
        .ejecutar(&admin(), propuesta)
        .await
        .unwrap();
    let comision = casos.parametros.comision_por_defecto(&mut ()).await.unwrap();
    assert_eq!(comision.centesimas(), 4550);
}
