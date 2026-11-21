//! Pruebas de la fachada con almacenamiento y repositorio en memoria (contexto `C = ()`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::TimeDelta;
use nucleo::identificadores::IdArchivo;
use nucleo::{ErrorPersistencia, RelojControlado, SinTransaccion};
use plataforma::seguridad::FirmadorUrls;

use super::ServicioArchivos;
use super::puertos::{Almacenamiento, NuevoRegistro, RegistroArchivo, RepositorioArchivos};
use crate::api::{ArchivoNuevo, Archivos, ErrorArchivos};
use crate::dominio::{ClaveArchivo, TipoArchivo, Visibilidad};

#[derive(Debug, Default)]
struct Disco(Mutex<HashMap<String, Vec<u8>>>);

#[async_trait]
impl Almacenamiento for Disco {
    async fn escribir(&self, clave: &ClaveArchivo, contenido: &[u8]) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert(clave.como_str().into(), contenido.to_vec());
        Ok(())
    }
    async fn leer(&self, clave: &ClaveArchivo) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(clave.como_str()).cloned())
    }
    async fn borrar(&self, clave: &ClaveArchivo) -> Result<(), String> {
        self.0.lock().unwrap().remove(clave.como_str());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Registros(Mutex<Vec<RegistroArchivo>>);

#[async_trait]
impl RepositorioArchivos<()> for Registros {
    async fn insertar(&self, _: &mut (), n: &NuevoRegistro) -> Result<IdArchivo, ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        let id = IdArchivo(i64::try_from(filas.len()).unwrap() + 1);
        filas.push(RegistroArchivo {
            id,
            clave: n.clave.clone(),
            nombre_original: n.nombre_original.clone(),
            tipo: n.tipo,
            tamano_bytes: n.tamano_bytes,
            visibilidad: n.visibilidad,
        });
        Ok(id)
    }
    async fn obtener(
        &self,
        _: &mut (),
        ids: &[IdArchivo],
    ) -> Result<Vec<RegistroArchivo>, ErrorPersistencia> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|r| ids.contains(&r.id))
            .cloned()
            .collect())
    }
    async fn eliminar(&self, _: &mut (), id: IdArchivo) -> Result<Option<ClaveArchivo>, ErrorPersistencia> {
        let mut filas = self.0.lock().unwrap();
        let posicion = filas.iter().position(|r| r.id == id);
        Ok(posicion.map(|i| filas.remove(i).clave))
    }
}

/// Cabecera PNG mínima con las medidas indicadas (suficiente para leer su tamaño).
pub(crate) fn png(ancho: u32, alto: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend_from_slice(&ancho.to_be_bytes());
    bytes.extend_from_slice(&alto.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

struct Escenario {
    servicio: ServicioArchivos<()>,
    disco: Arc<Disco>,
    reloj: Arc<RelojControlado>,
}

fn escenario() -> Escenario {
    let disco = Arc::new(Disco::default());
    let reloj = Arc::new(RelojControlado::en("2026-11-20T15:00:00Z".parse().unwrap()));
    let servicio = ServicioArchivos {
        transacciones: Arc::new(SinTransaccion),
        repositorio: Arc::new(Registros::default()),
        almacenamiento: disco.clone(),
        firmador: FirmadorUrls::nuevo(b"clave de pruebas"),
        reloj: reloj.clone(),
    };
    Escenario {
        servicio,
        disco,
        reloj,
    }
}

fn foto(contenido: Vec<u8>) -> ArchivoNuevo {
    ArchivoNuevo {
        campo: "foto",
        contenido,
        nombre_original: "santiago.png".into(),
        visibilidad: Visibilidad::Publico,
        carpeta: "barberos/fotos",
        tipos_permitidos: TipoArchivo::IMAGENES,
        tamano_maximo: 1024,
        exigir_vertical: true,
        subido_por: None,
    }
}

fn documento() -> ArchivoNuevo {
    ArchivoNuevo {
        campo: "archivo",
        contenido: b"%PDF-1.7 contrato".to_vec(),
        nombre_original: "C:\\docs\\contrato firmado.pdf".into(),
        visibilidad: Visibilidad::Privado,
        carpeta: "barberos/documentos",
        tipos_permitidos: TipoArchivo::DOCUMENTOS,
        tamano_maximo: 1024,
        exigir_vertical: false,
        subido_por: None,
    }
}

#[tokio::test]
async fn guarda_una_foto_publica_con_clave_aleatoria() {
    let e = escenario();
    let guardado = e.servicio.guardar(&mut (), foto(png(300, 400))).await.unwrap();
    assert_eq!(guardado.tipo, TipoArchivo::Png);
    assert!(guardado.clave.como_str().starts_with("publico/barberos/fotos/"));
    assert!(e.disco.0.lock().unwrap().contains_key(guardado.clave.como_str()));

    let urls = e.servicio.urls_publicas(&mut (), &[guardado.id]).await.unwrap();
    assert_eq!(urls[&guardado.id], format!("/media/{}", guardado.clave));
    let (tipo, _) = e
        .servicio
        .leer_publico(guardado.clave.como_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tipo, TipoArchivo::Png);
}

#[tokio::test]
async fn valida_contenido_tamano_y_orientacion() {
    let e = escenario();
    type Esperado = fn(&ErrorArchivos) -> bool;
    let casos: Vec<(ArchivoNuevo, Esperado)> = vec![
        (foto(Vec::new()), |e| matches!(e, ErrorArchivos::Vacio { .. })),
        (foto([0xFF, 0xD8, 0xFF].repeat(400)), |e| {
            matches!(e, ErrorArchivos::DemasiadoGrande { .. })
        }),
        (foto(b"%PDF-1.7".to_vec()), |e| {
            matches!(e, ErrorArchivos::TipoNoPermitido { .. })
        }),
        (foto(b"<svg onload=alert(1)>".to_vec()), |e| {
            matches!(e, ErrorArchivos::TipoNoPermitido { .. })
        }),
        (foto(png(400, 300)), |e| {
            matches!(e, ErrorArchivos::NoVertical { .. })
        }),
    ];
    for (nuevo, esperado) in casos {
        let error = e.servicio.guardar(&mut (), nuevo).await.unwrap_err();
        assert!(esperado(&error), "{error:?}");
        assert_eq!(
            nucleo::ErrorDeAplicacion::errores_de_campo(&error)[0].campo,
            "foto"
        );
    }
    assert!(
        e.disco.0.lock().unwrap().is_empty(),
        "nada inválido llega al almacenamiento"
    );
}

#[tokio::test]
async fn los_privados_solo_se_descargan_con_una_url_firmada_vigente() {
    let e = escenario();
    let guardado = e.servicio.guardar(&mut (), documento()).await.unwrap();
    assert_eq!(guardado.nombre_original, "contrato firmado.pdf");
    assert!(
        e.servicio
            .urls_publicas(&mut (), &[guardado.id])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        e.servicio
            .leer_publico(guardado.clave.como_str())
            .await
            .unwrap()
            .is_none()
    );

    let url = e.servicio.url_firmada(guardado.id, Duration::from_secs(300));
    let (exp, firma) = parametros(&url);
    let descarga = e.servicio.descargar(guardado.id, exp, &firma).await.unwrap();
    assert_eq!(
        (descarga.tipo, descarga.contenido.as_slice()),
        (TipoArchivo::Pdf, &b"%PDF-1.7 contrato"[..])
    );

    assert!(matches!(
        e.servicio.descargar(IdArchivo(99), exp, &firma).await,
        Err(ErrorArchivos::FirmaInvalida)
    ));
    assert!(matches!(
        e.servicio.descargar(guardado.id, exp + 60, &firma).await,
        Err(ErrorArchivos::FirmaInvalida)
    ));
    e.reloj.adelantar(TimeDelta::minutes(6));
    assert!(matches!(
        e.servicio.descargar(guardado.id, exp, &firma).await,
        Err(ErrorArchivos::EnlaceVencido)
    ));
}

#[tokio::test]
async fn eliminar_devuelve_la_clave_para_borrar_el_contenido_despues() {
    let e = escenario();
    let guardado = e.servicio.guardar(&mut (), documento()).await.unwrap();
    let clave = e.servicio.eliminar(&mut (), guardado.id).await.unwrap().unwrap();
    assert!(
        e.disco.0.lock().unwrap().contains_key(clave.como_str()),
        "el contenido se borra tras confirmar"
    );
    e.servicio.borrar_contenido(&clave).await;
    assert!(e.disco.0.lock().unwrap().is_empty());
    assert_eq!(e.servicio.eliminar(&mut (), guardado.id).await.unwrap(), None);
}

fn parametros(url: &str) -> (i64, String) {
    let consulta = url.split_once('?').unwrap().1;
    let mut exp = 0;
    let mut firma = String::new();
    for par in consulta.split('&') {
        match par.split_once('=').unwrap() {
            ("exp", v) => exp = v.parse().unwrap(),
            ("firma", v) => firma = v.to_owned(),
            _ => {}
        }
    }
    (exp, firma)
}
