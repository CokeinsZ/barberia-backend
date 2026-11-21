//! Pruebas de los casos de uso con dobles en memoria (contexto `C = ()`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use auditoria::api::{Auditor, EntradaAuditoria};
use nucleo::identificadores::{IdBarbero, IdCategoria, IdServicio, IdUsuario};
use nucleo::{Actor, ErrorPersistencia, SinTransaccion};

use super::CasosDeUso;
use super::comandos::NuevoServicio;
use super::dto::{CategoriaDto, ComponenteDto};
use super::errores::ErrorCatalogo;
use super::puertos::{Puertos, RESTRICCION_NOMBRE, RepositorioServicios, ServicioGuardado};
use crate::api::{CatalogoServicios, OferentesServicio};
use crate::dominio::{DatosServicio, PropuestaServicio};

#[derive(Debug, Default)]
struct Servicios(Mutex<Vec<ServicioGuardado>>);

#[async_trait]
impl RepositorioServicios<()> for Servicios {
    async fn categorias(&self, _: &mut ()) -> Result<Vec<CategoriaDto>, ErrorPersistencia> {
        Ok(["Cabello", "Barba", "Combos", "Complementos"]
            .iter()
            .zip(1..)
            .map(|(n, i)| CategoriaDto {
                id: IdCategoria(i),
                nombre: (*n).into(),
                orden: i16::try_from(i).unwrap(),
            })
            .collect())
    }
    async fn listar(
        &self,
        _: &mut (),
        incluir_inactivos: bool,
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| incluir_inactivos || s.activo)
            .cloned()
            .collect())
    }
    async fn obtener(
        &self,
        _: &mut (),
        ids: &[IdServicio],
    ) -> Result<Vec<ServicioGuardado>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| ids.contains(&s.id))
            .cloned()
            .collect())
    }
    async fn crear(&self, _: &mut (), d: &DatosServicio) -> Result<IdServicio, ErrorPersistencia> {
        let mut todos = self.0.lock().unwrap();
        if todos
            .iter()
            .any(|s| s.datos.nombre.to_lowercase() == d.nombre.to_lowercase())
        {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: RESTRICCION_NOMBRE.into(),
            });
        }
        let id = IdServicio(i64::try_from(todos.len()).unwrap() + 1);
        todos.push(ServicioGuardado {
            id,
            datos: d.clone(),
            activo: true,
            categoria_nombre: "Categoría".into(),
            componentes: Vec::new(),
        });
        Ok(id)
    }
    async fn actualizar(
        &self,
        _: &mut (),
        id: IdServicio,
        d: &DatosServicio,
    ) -> Result<(), ErrorPersistencia> {
        let mut todos = self.0.lock().unwrap();
        if todos
            .iter()
            .any(|s| s.id != id && s.datos.nombre.to_lowercase() == d.nombre.to_lowercase())
        {
            return Err(ErrorPersistencia::Duplicado {
                restriccion: RESTRICCION_NOMBRE.into(),
            });
        }
        todos.iter_mut().find(|s| s.id == id).unwrap().datos = d.clone();
        Ok(())
    }
    async fn cambiar_estado(
        &self,
        _: &mut (),
        id: IdServicio,
        activo: bool,
    ) -> Result<(), ErrorPersistencia> {
        self.0
            .lock()
            .unwrap()
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .activo = activo;
        Ok(())
    }
    async fn reemplazar_componentes(
        &self,
        _: &mut (),
        combo: IdServicio,
        ids: &[IdServicio],
    ) -> Result<(), ErrorPersistencia> {
        let mut todos = self.0.lock().unwrap();
        let nombres: HashMap<IdServicio, String> =
            todos.iter().map(|s| (s.id, s.datos.nombre.clone())).collect();
        todos.iter_mut().find(|s| s.id == combo).unwrap().componentes = ids
            .iter()
            .map(|id| ComponenteDto {
                id: *id,
                nombre: nombres[id].clone(),
            })
            .collect();
        Ok(())
    }
}

/// Barberos que ofrecen cada servicio, fijados por la prueba.
#[derive(Debug, Default)]
struct Oferentes(Mutex<HashMap<IdServicio, i64>>);

#[async_trait]
impl OferentesServicio<()> for Oferentes {
    async fn contar(&self, _: &mut ()) -> Result<HashMap<IdServicio, i64>, ErrorPersistencia> {
        Ok(self.0.lock().unwrap().clone())
    }
}

#[derive(Debug, Default)]
struct Auditoria(Mutex<Vec<&'static str>>);

#[async_trait]
impl Auditor<()> for Auditoria {
    async fn registrar(&self, _: &mut (), e: EntradaAuditoria) -> Result<(), ErrorPersistencia> {
        self.0.lock().unwrap().push(e.accion);
        Ok(())
    }
}

struct Escenario {
    casos: CasosDeUso<()>,
    oferentes: Arc<Oferentes>,
    auditoria: Arc<Auditoria>,
}

fn escenario() -> Escenario {
    let oferentes = Arc::new(Oferentes::default());
    let auditoria = Arc::new(Auditoria::default());
    let casos = CasosDeUso::nuevos(Puertos {
        transacciones: Arc::new(SinTransaccion),
        servicios: Arc::new(Servicios::default()),
        oferentes: oferentes.clone(),
        auditor: auditoria.clone(),
    });
    Escenario {
        casos,
        oferentes,
        auditoria,
    }
}

const ADMIN: Actor = Actor::Administrador {
    usuario: IdUsuario(1),
};
const BARBERO: Actor = Actor::Barbero {
    usuario: IdUsuario(2),
    barbero: IdBarbero(1),
};

fn simple(nombre: &str, precio: i64) -> NuevoServicio {
    NuevoServicio {
        propuesta: PropuestaServicio {
            tipo: "simple".into(),
            categoria_id: 1,
            nombre: nombre.into(),
            descripcion: format!("{nombre} profesional."),
            precio,
            duracion_min: Some(30),
            agendable_en_linea: true,
            visible_en_landing: true,
            ..Default::default()
        },
        componentes: Vec::new(),
    }
}

fn combo(componentes: &[i64]) -> NuevoServicio {
    let mut nuevo = simple("Corte + cejas", 40_000);
    nuevo.propuesta.tipo = "combo".into();
    nuevo.propuesta.categoria_id = 3;
    nuevo.propuesta.duracion_min = Some(60);
    nuevo.componentes = componentes.iter().copied().map(IdServicio).collect();
    nuevo
}

fn campos(error: ErrorCatalogo) -> Vec<String> {
    match error {
        ErrorCatalogo::DatosInvalidos(e) => e.into_iter().map(|e| e.campo).collect(),
        otro => panic!("se esperaba DatosInvalidos y llegó {otro:?}"),
    }
}

#[tokio::test]
async fn crea_servicios_y_un_combo_con_precio_propio() {
    let e = escenario();
    let corte = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Corte clásico", 35_000))
        .await
        .unwrap();
    let cejas = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Diseño de cejas", 10_000))
        .await
        .unwrap();
    let combo = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, combo(&[corte.id.0, cejas.id.0]))
        .await
        .unwrap();
    assert_eq!((combo.tipo, combo.precio.valor()), ("combo", 40_000));
    let nombres: Vec<&str> = combo.componentes.iter().map(|c| c.nombre.as_str()).collect();
    assert_eq!(nombres, ["Corte clásico", "Diseño de cejas"]);
    assert_eq!(*e.auditoria.0.lock().unwrap(), ["crear_servicio"; 3]);
}

#[tokio::test]
async fn valida_los_componentes_del_combo() {
    let e = escenario();
    let corte = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Corte clásico", 35_000))
        .await
        .unwrap();
    let cejas = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Diseño de cejas", 10_000))
        .await
        .unwrap();
    assert_eq!(
        campos(
            e.casos
                .crear_servicio
                .ejecutar(&ADMIN, combo(&[corte.id.0]))
                .await
                .unwrap_err()
        ),
        ["componentes"]
    );
    assert_eq!(
        campos(
            e.casos
                .crear_servicio
                .ejecutar(&ADMIN, combo(&[corte.id.0, 99]))
                .await
                .unwrap_err()
        ),
        ["componentes"]
    );

    e.casos
        .cambiar_estado
        .ejecutar(&ADMIN, cejas.id, false)
        .await
        .unwrap();
    let con_inactivo = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, combo(&[corte.id.0, cejas.id.0]))
        .await;
    assert_eq!(
        campos(con_inactivo.unwrap_err()),
        ["componentes"],
        "un componente inactivo no sirve"
    );

    let mut simple_con_componentes = simple("Barba", 20_000);
    simple_con_componentes.componentes = vec![corte.id];
    assert_eq!(
        campos(
            e.casos
                .crear_servicio
                .ejecutar(&ADMIN, simple_con_componentes)
                .await
                .unwrap_err()
        ),
        ["componentes"]
    );
    assert!(matches!(
        e.casos
            .reemplazar_componentes
            .ejecutar(&ADMIN, corte.id, vec![cejas.id])
            .await,
        Err(ErrorCatalogo::NoEsCombo)
    ));
}

#[tokio::test]
async fn el_nombre_es_unico_y_el_tipo_no_cambia() {
    let e = escenario();
    let corte = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Corte clásico", 35_000))
        .await
        .unwrap();
    assert!(matches!(
        e.casos
            .crear_servicio
            .ejecutar(&ADMIN, simple("CORTE CLÁSICO", 1))
            .await,
        Err(ErrorCatalogo::NombreEnUso)
    ));
    let mut a_combo = simple("Corte clásico", 38_000).propuesta;
    a_combo.tipo = "combo".into();
    assert_eq!(
        campos(
            e.casos
                .editar_servicio
                .ejecutar(&ADMIN, corte.id, a_combo)
                .await
                .unwrap_err()
        ),
        ["tipo"]
    );

    let mut nuevo_precio = simple("Corte clásico", 38_000).propuesta;
    nuevo_precio.tipo = String::new();
    let editado = e
        .casos
        .editar_servicio
        .ejecutar(&ADMIN, corte.id, nuevo_precio)
        .await
        .unwrap();
    assert_eq!(editado.precio.valor(), 38_000);
    let mut otra_categoria = simple("Corte clásico", 38_000).propuesta;
    otra_categoria.categoria_id = 9;
    assert_eq!(
        campos(
            e.casos
                .editar_servicio
                .ejecutar(&ADMIN, corte.id, otra_categoria)
                .await
                .unwrap_err()
        ),
        ["categoria_id"]
    );
}

#[tokio::test]
async fn advierte_cuando_ningun_barbero_ofrece_el_servicio_y_oculta_los_inactivos() {
    let e = escenario();
    let corte = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Corte clásico", 35_000))
        .await
        .unwrap();
    assert!(corte.advertencia.is_some());
    e.oferentes.0.lock().unwrap().insert(corte.id, 2);
    let lista = e.casos.listar_servicios.ejecutar(&BARBERO, false).await.unwrap();
    assert_eq!(
        (lista[0].barberos_que_lo_ofrecen, lista[0].advertencia),
        (2, None)
    );

    e.casos
        .cambiar_estado
        .ejecutar(&ADMIN, corte.id, false)
        .await
        .unwrap();
    assert!(
        e.casos
            .listar_servicios
            .ejecutar(&BARBERO, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.casos
            .listar_servicios
            .ejecutar(&ADMIN, true)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(e.auditoria.0.lock().unwrap().contains(&"desactivar_servicio"));
}

#[tokio::test]
async fn solo_el_administrador_cambia_el_catalogo_y_la_fachada_resume() {
    let e = escenario();
    assert!(matches!(
        e.casos
            .crear_servicio
            .ejecutar(&BARBERO, simple("Corte", 1))
            .await,
        Err(ErrorCatalogo::Prohibido)
    ));
    let corte = e
        .casos
        .crear_servicio
        .ejecutar(&ADMIN, simple("Corte clásico", 35_000))
        .await
        .unwrap();
    let resumen = e
        .casos
        .fachada
        .resumenes(&mut (), &[corte.id, IdServicio(42)])
        .await
        .unwrap();
    assert_eq!(resumen.len(), 1);
    assert_eq!((resumen[0].duracion_min, resumen[0].activo), (Some(30), true));
    assert!(matches!(
        e.casos.obtener_servicio.ejecutar(&BARBERO, IdServicio(42)).await,
        Err(ErrorCatalogo::ServicioNoEncontrado)
    ));
}
