use std::collections::BTreeSet;
use std::time::Duration;

use archivos::api::{ArchivoNuevo, TipoArchivo, Visibilidad};
use nucleo::Actor;
use nucleo::identificadores::{IdBarbero, IdDocumento};
use plataforma::http::ArchivoSubido;
use serde_json::{Value, json};

use crate::aplicacion::caso_de_uso;
use crate::aplicacion::dto::{ContactoEmergenciaDto, DatosPrivadosDto, DocumentoDto, ExpedienteDto, FotoDto};
use crate::aplicacion::errores::ErrorBarberos;
use crate::aplicacion::politicas::solo_administrador;
use crate::aplicacion::puertos::{
    DocumentoGuardado, NuevoDocumento, PrivadosGuardados, Puertos, RESTRICCION_PLANILLA,
};
use crate::dominio::{DatosPrivados, PropuestaPrivados, TipoAdjunto, periodo_planilla};

const VIGENCIA_DESCARGA: Duration = Duration::from_secs(5 * 60);
const TAMANO_MAXIMO_FOTO: usize = 5 * 1024 * 1024;
const TAMANO_MAXIMO_DOCUMENTO: usize = 10 * 1024 * 1024;

caso_de_uso!(
    /// Foto del barbero (RF-BAR-04): imagen vertical, pública (la muestra la landing).
    CambiarFoto
);

impl<C: Send + 'static> CambiarFoto<C> {
    pub(crate) async fn ejecutar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        foto: Option<ArchivoSubido>,
    ) -> Result<FotoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let foto = foto.ok_or(archivos::api::ErrorArchivos::Vacio { campo: "foto" })?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let guardado = p
            .archivos
            .guardar(
                &mut cx,
                ArchivoNuevo {
                    campo: "foto",
                    contenido: foto.contenido,
                    nombre_original: foto.nombre,
                    visibilidad: Visibilidad::Publico,
                    carpeta: "barberos/fotos",
                    tipos_permitidos: TipoArchivo::IMAGENES,
                    tamano_maximo: TAMANO_MAXIMO_FOTO,
                    exigir_vertical: true,
                    subido_por: actor.usuario(),
                },
            )
            .await?;
        let anterior = p.barberos.cambiar_foto(&mut cx, id, guardado.id).await?;
        let clave_anterior = match anterior {
            Some(vieja) => p.archivos.eliminar(&mut cx, vieja).await?,
            None => None,
        };
        let dto = FotoDto {
            foto_url: guardado.clave.url_publica(),
        };
        p.auditar(&mut cx, p.entrada(actor, "cambiar_foto", id).despues(&dto))
            .await?;
        p.transacciones.confirmar(cx).await?;
        if let Some(clave) = clave_anterior {
            p.archivos.borrar_contenido(&clave).await;
        }
        Ok(dto)
    }
}

caso_de_uso!(
    /// Documentos del barbero (RF-BAR-06, RF-BAR-18): privados, descargables solo con una URL
    /// firmada de 5 minutos (RNF-SEG-09).
    GestionDocumentos
);

impl<C: Send + 'static> GestionDocumentos<C> {
    async fn dtos(
        p: &Puertos<C>,
        cx: &mut C,
        documentos: Vec<DocumentoGuardado>,
    ) -> Result<Vec<DocumentoDto>, ErrorBarberos> {
        let ids: Vec<_> = documentos.iter().map(|d| d.archivo).collect();
        let metadatos = p.archivos.metadatos(cx, &ids).await?;
        Ok(documentos
            .into_iter()
            .filter_map(|d| {
                let m = metadatos.iter().find(|m| m.id == d.archivo)?;
                Some(DocumentoDto {
                    id: d.id,
                    tipo: d.tipo,
                    periodo: d.periodo,
                    nombre_original: m.nombre_original.clone(),
                    tamano_bytes: m.tamano_bytes,
                    cargado_en: d.cargado_en,
                    url_descarga: p.archivos.url_firmada(d.archivo, VIGENCIA_DESCARGA),
                })
            })
            .collect())
    }

    pub(crate) async fn listar(&self, actor: &Actor, id: IdBarbero) -> Result<ExpedienteDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let guardados = p.expediente.documentos(&mut cx, id).await?;
        let documentos = Self::dtos(p, &mut cx, guardados).await?;
        p.transacciones.confirmar(cx).await?;
        let presentes: BTreeSet<&str> = documentos.iter().map(|d| d.tipo.as_str()).collect();
        let faltantes: Vec<&'static str> = TipoAdjunto::REQUERIDOS
            .iter()
            .map(|t| t.codigo())
            .filter(|t| !presentes.contains(t))
            .collect();
        Ok(ExpedienteDto {
            completo: faltantes.is_empty(),
            documentos,
            faltantes,
        })
    }

    pub(crate) async fn cargar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        tipo: Option<&str>,
        periodo: Option<&str>,
        archivo: Option<ArchivoSubido>,
    ) -> Result<DocumentoDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let tipo = tipo.and_then(TipoAdjunto::desde_codigo).ok_or_else(|| {
            ErrorBarberos::DatosInvalidos(vec![nucleo::ErrorDeCampo::nuevo(
                "tipo",
                "Elige identidad, contrato, hoja_de_vida, certificado, afiliacion o planilla_pila.",
            )])
        })?;
        let periodo = periodo_planilla(tipo, periodo)?;
        let archivo = archivo.ok_or(archivos::api::ErrorArchivos::Vacio { campo: "archivo" })?;
        let p = &self.p;
        let por = actor.usuario().ok_or(ErrorBarberos::Prohibido)?;
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let guardado = p
            .archivos
            .guardar(
                &mut cx,
                ArchivoNuevo {
                    campo: "archivo",
                    contenido: archivo.contenido,
                    nombre_original: archivo.nombre,
                    visibilidad: Visibilidad::Privado,
                    carpeta: "barberos/documentos",
                    tipos_permitidos: TipoArchivo::DOCUMENTOS,
                    tamano_maximo: TAMANO_MAXIMO_DOCUMENTO,
                    exigir_vertical: false,
                    subido_por: Some(por),
                },
            )
            .await?;
        let ahora = p.reloj.ahora();
        let did = match p
            .expediente
            .agregar_documento(
                &mut cx,
                id,
                &NuevoDocumento {
                    tipo,
                    periodo,
                    archivo: guardado.id,
                    por,
                    ahora,
                },
            )
            .await
        {
            Ok(did) => did,
            Err(nucleo::ErrorPersistencia::Duplicado { restriccion })
                if restriccion == RESTRICCION_PLANILLA =>
            {
                return Err(ErrorBarberos::PlanillaRepetida);
            }
            Err(otro) => return Err(otro.into()),
        };
        let documento = DocumentoGuardado {
            id: did,
            tipo: tipo.codigo().to_owned(),
            periodo,
            archivo: guardado.id,
            cargado_en: ahora,
        };
        let dto = Self::dtos(p, &mut cx, vec![documento])
            .await?
            .pop()
            .ok_or_else(|| ErrorBarberos::Interno("documento sin archivo".into()))?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "cargar_documento", id)
                .despues(&json!({ "documento": did, "tipo": tipo.codigo(), "periodo": periodo })),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(dto)
    }

    pub(crate) async fn quitar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        documento: IdDocumento,
    ) -> Result<(), ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let quitado = p
            .expediente
            .quitar_documento(&mut cx, id, documento)
            .await?
            .ok_or(ErrorBarberos::RecursoNoEncontrado("El documento"))?;
        let clave = p.archivos.eliminar(&mut cx, quitado.archivo).await?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "quitar_documento", id)
                .antes(&json!({ "documento": documento, "tipo": quitado.tipo })),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        if let Some(clave) = clave {
            p.archivos.borrar_contenido(&clave).await;
        }
        Ok(())
    }

    /// URL firmada para descargar un documento (la ruta HTTP redirige a ella).
    pub(crate) async fn url_descarga(
        &self,
        actor: &Actor,
        id: IdBarbero,
        documento: IdDocumento,
    ) -> Result<String, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        let encontrado = p.expediente.documento(&mut cx, id, documento).await?;
        p.transacciones.confirmar(cx).await?;
        let encontrado = encontrado.ok_or(ErrorBarberos::RecursoNoEncontrado("El documento"))?;
        Ok(p.archivos.url_firmada(encontrado.archivo, VIGENCIA_DESCARGA))
    }
}

caso_de_uso!(
    /// Datos complementarios (RF-BAR-05). Solo los ve el administrador; el grupo sanguíneo y el
    /// número de cuenta se guardan cifrados (RNF-LEG-04) y la auditoría registra qué campos
    /// cambiaron, nunca sus valores.
    GestionDatosPrivados
);

impl<C: Send + 'static> GestionDatosPrivados<C> {
    fn descifrar(
        p: &Puertos<C>,
        guardados: Option<PrivadosGuardados>,
    ) -> Result<DatosPrivadosDto, ErrorBarberos> {
        let g = guardados.unwrap_or_default();
        let descifrar = |dato: Option<Vec<u8>>| -> Result<Option<String>, ErrorBarberos> {
            dato.map(|d| p.cifrador.descifrar(&d))
                .transpose()
                .map_err(|e| ErrorBarberos::Interno(e.to_string()))
        };
        Ok(DatosPrivadosDto {
            direccion: g.direccion,
            ciudad: g.ciudad,
            grupo_sanguineo: descifrar(g.grupo_sanguineo_cifrado)?,
            emergencia: ContactoEmergenciaDto {
                nombre: g.emergencia_nombre,
                parentesco: g.emergencia_parentesco,
                celular: g.emergencia_celular,
            },
            eps: g.eps,
            fondo_pensiones: g.fondo_pensiones,
            arl: g.arl,
            caja_compensacion: g.caja_compensacion,
            medio_pago: g.medio_pago,
            banco: g.banco,
            tipo_cuenta: g.tipo_cuenta,
            numero_cuenta: descifrar(g.numero_cuenta_cifrado)?,
        })
    }

    pub(crate) async fn obtener(
        &self,
        actor: &Actor,
        id: IdBarbero,
    ) -> Result<DatosPrivadosDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let p = &self.p;
        let mut cx = p.transacciones.iniciar().await?;
        p.guardado(&mut cx, id).await?;
        let guardados = p.expediente.datos_privados(&mut cx, id).await?;
        p.transacciones.confirmar(cx).await?;
        Self::descifrar(p, guardados)
    }

    pub(crate) async fn guardar(
        &self,
        actor: &Actor,
        id: IdBarbero,
        propuesta: PropuestaPrivados,
    ) -> Result<DatosPrivadosDto, ErrorBarberos> {
        solo_administrador(actor)?;
        let datos = DatosPrivados::nuevos(propuesta)?;
        let p = &self.p;
        let cifrar = |dato: &Option<String>| -> Result<Option<Vec<u8>>, ErrorBarberos> {
            dato.as_deref()
                .map(|d| p.cifrador.cifrar(d))
                .transpose()
                .map_err(|e| ErrorBarberos::Interno(e.to_string()))
        };
        let guardados = PrivadosGuardados {
            direccion: datos.direccion.clone(),
            ciudad: datos.ciudad.clone(),
            grupo_sanguineo_cifrado: cifrar(&datos.grupo_sanguineo)?,
            emergencia_nombre: datos.emergencia_nombre.clone(),
            emergencia_parentesco: datos.emergencia_parentesco.map(str::to_owned),
            emergencia_celular: datos.emergencia_celular.as_ref().map(|c| c.digitos().to_owned()),
            eps: datos.eps.clone(),
            fondo_pensiones: datos.fondo_pensiones.clone(),
            arl: datos.arl.clone(),
            caja_compensacion: datos.caja_compensacion.clone(),
            medio_pago: datos.medio_pago.map(str::to_owned),
            banco: datos.banco.clone(),
            tipo_cuenta: datos.tipo_cuenta.clone(),
            numero_cuenta_cifrado: cifrar(&datos.numero_cuenta)?,
        };
        let mut cx = p.transacciones.iniciar().await?;
        if !p.barberos.bloquear(&mut cx, id).await? {
            return Err(ErrorBarberos::NoEncontrado);
        }
        let antes = Self::descifrar(p, p.expediente.datos_privados(&mut cx, id).await?)?;
        p.expediente
            .guardar_privados(&mut cx, id, &guardados, p.reloj.ahora())
            .await?;
        let despues = Self::descifrar(p, Some(guardados))?;
        p.auditar(
            &mut cx,
            p.entrada(actor, "actualizar_datos_privados", id)
                .despues(&json!({ "campos": campos_cambiados(&antes, &despues) })),
        )
        .await?;
        p.transacciones.confirmar(cx).await?;
        Ok(despues)
    }
}

/// Nombres de los campos que cambiaron (sin sus valores).
fn campos_cambiados(antes: &DatosPrivadosDto, despues: &DatosPrivadosDto) -> Vec<String> {
    let (Ok(Value::Object(a)), Ok(Value::Object(d))) =
        (serde_json::to_value(antes), serde_json::to_value(despues))
    else {
        return Vec::new();
    };
    d.iter()
        .filter(|(campo, valor)| a.get(*campo) != Some(valor))
        .map(|(campo, _)| campo.clone())
        .collect()
}
