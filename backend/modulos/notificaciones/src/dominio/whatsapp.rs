//! Plantillas de WhatsApp: el texto registrado en Meta (español de Colombia, `es_CO`; ver
//! `docs/diseno/plantillas-whatsapp.md`) y sus parámetros. A Meta solo van los parámetros, en
//! orden: Meta arma el mensaje con su texto. El simulador muestra este mismo texto, así lo que
//! se prueba es lo que recibirá el cliente.

use chrono::TimeDelta;

use crate::api::PlantillaWhatsApp;

/// Carga útil de los botones de respuesta rápida del recordatorio.
pub(crate) const CONFIRMAR: &str = "confirmar";
pub(crate) const CANCELAR: &str = "cancelar";

/// Botón de respuesta rápida: el texto que ve el cliente y la carga que vuelve en el webhook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BotonRespuesta {
    pub(crate) texto: &'static str,
    pub(crate) carga: &'static str,
}

/// Definición de una plantilla tal como se registra en Meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefinicionPlantilla {
    pub(crate) nombre: &'static str,
    /// `AUTHENTICATION` o `UTILITY`.
    pub(crate) categoria: &'static str,
    /// Cuerpo con las variables `{{1}}`, `{{2}}`… (Meta no admite variables al inicio ni al final).
    pub(crate) cuerpo: &'static str,
    pub(crate) botones: &'static [BotonRespuesta],
}

const BOTONES_RECORDATORIO: &[BotonRespuesta] = &[
    BotonRespuesta {
        texto: "Confirmo",
        carga: CONFIRMAR,
    },
    BotonRespuesta {
        texto: "Cancelar cita",
        carga: CANCELAR,
    },
];

pub(crate) const DEFINICIONES: [DefinicionPlantilla; 6] = [
    DefinicionPlantilla {
        nombre: "codigo_verificacion",
        categoria: "AUTHENTICATION",
        cuerpo: "*{{1}}* es tu código de verificación. Por tu seguridad, no lo compartas. Este código caduca en 10 minutos.",
        botones: &[],
    },
    DefinicionPlantilla {
        nombre: "cita_confirmada",
        categoria: "UTILITY",
        cuerpo: "Hola {{1}}.\nTu cita en {{2}} para el servicio {{3}} quedó agendada con {{4}} el {{5}} a las {{6}}.\n\n\
                 Puedes verla, reprogramarla o cancelarla aquí: {{7}}\n\n¡Te esperamos!",
        botones: &[],
    },
    DefinicionPlantilla {
        nombre: "recordatorio_cita",
        categoria: "UTILITY",
        cuerpo: "Hola {{1}}.\nTe recordamos tu cita en {{2}} el {{3}} a las {{4}} con {{5}}.\n¿Nos confirmas tu asistencia?",
        botones: BOTONES_RECORDATORIO,
    },
    DefinicionPlantilla {
        nombre: "cita_reprogramada",
        categoria: "UTILITY",
        cuerpo: "Hola {{1}}.\nTu cita en {{2}} cambió.\nAhora es el {{3}} a las {{4}}, con {{5}}.\n\n\
                 Puedes gestionarla aquí: {{6}}\n\n¡Te esperamos!",
        botones: &[],
    },
    DefinicionPlantilla {
        nombre: "cita_cancelada",
        categoria: "UTILITY",
        cuerpo: "Hola {{1}}.\nTu cita en {{2}} de {{3}} de la fecha {{4}} fue cancelada.\n\
                 Si quieres, puedes agendar otra en nuestra página.\n¡Gracias!",
        botones: &[],
    },
    DefinicionPlantilla {
        nombre: "aviso_reprogramacion",
        categoria: "UTILITY",
        cuerpo: "Hola {{1}}, por un imprevisto no podremos atenderte el {{2}} a las {{3}} en tu cita de {{4}}. \
                 Elige otra hora aquí: {{5}} Disculpa las molestias.",
        botones: &[],
    },
];

/// Mensaje listo para el proveedor: plantilla, parámetros en orden y el texto resultante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WhatsAppRenderizado {
    pub(crate) plantilla: &'static str,
    pub(crate) parametros: Vec<String>,
    pub(crate) botones: &'static [BotonRespuesta],
    pub(crate) texto: String,
}

pub(crate) fn definicion(nombre: &str) -> Option<&'static DefinicionPlantilla> {
    DEFINICIONES.iter().find(|d| d.nombre == nombre)
}

/// Parámetros de cada plantilla, en el orden de sus variables. `negocio` es el nombre
/// comercial (variable `{{2}}` de las plantillas de citas): se toma al enviar, así un cambio de
/// nombre en la configuración se ve en los mensajes siguientes.
pub(crate) fn renderizar(plantilla: &PlantillaWhatsApp, negocio: &str) -> WhatsAppRenderizado {
    let negocio = negocio.to_owned();
    let (nombre, parametros): (&str, Vec<String>) = match plantilla {
        PlantillaWhatsApp::CodigoVerificacion { codigo } => ("codigo_verificacion", vec![codigo.clone()]),
        PlantillaWhatsApp::CitaConfirmada {
            nombre,
            servicio,
            barbero,
            fecha,
            hora,
            enlace,
        } => (
            "cita_confirmada",
            vec![
                nombre.clone(),
                negocio,
                servicio.clone(),
                barbero.clone(),
                fecha.clone(),
                hora.clone(),
                enlace.clone(),
            ],
        ),
        PlantillaWhatsApp::RecordatorioCita {
            nombre,
            barbero,
            fecha,
            hora,
            ..
        } => (
            "recordatorio_cita",
            vec![
                nombre.clone(),
                negocio,
                fecha.clone(),
                hora.clone(),
                barbero.clone(),
            ],
        ),
        PlantillaWhatsApp::CitaReprogramada {
            nombre,
            barbero,
            fecha,
            hora,
            enlace,
            ..
        } => (
            "cita_reprogramada",
            vec![
                nombre.clone(),
                negocio,
                fecha.clone(),
                hora.clone(),
                barbero.clone(),
                enlace.clone(),
            ],
        ),
        PlantillaWhatsApp::CitaCancelada {
            nombre,
            servicio,
            fecha,
            hora,
        } => (
            "cita_cancelada",
            // «…de la fecha {{4}} fue cancelada»: la fecha con su hora.
            vec![
                nombre.clone(),
                negocio,
                servicio.clone(),
                format!("{fecha} a las {hora}"),
            ],
        ),
        PlantillaWhatsApp::AvisoReprogramacion {
            nombre,
            servicio,
            fecha,
            hora,
            enlace,
        } => (
            "aviso_reprogramacion",
            vec![
                nombre.clone(),
                fecha.clone(),
                hora.clone(),
                servicio.clone(),
                enlace.clone(),
            ],
        ),
    };
    // Las definiciones cubren todas las variantes (lo verifica una prueba).
    let definicion = definicion(nombre).unwrap_or(&DEFINICIONES[0]);
    let parametros: Vec<String> = parametros
        .into_iter()
        .enumerate()
        .map(|(i, valor)| parametro(definicion.cuerpo, i + 1, &valor))
        .collect();
    let mut texto = definicion.cuerpo.to_owned();
    for (posicion, valor) in parametros.iter().enumerate() {
        texto = texto.replace(&format!("{{{{{}}}}}", posicion + 1), valor);
    }
    WhatsAppRenderizado {
        plantilla: definicion.nombre,
        parametros,
        botones: definicion.botones,
        texto,
    }
}

/// Cuánto puede esperar un WhatsApp en la cola antes de dejar de tener sentido: el código de
/// verificación vence a los 10 minutos (lo dice su plantilla); una confirmación, un
/// recordatorio o un aviso, a las 24 horas. Así un atraso largo, o configurar Meta sobre una cola
/// vieja (por ejemplo, la de preproducción, con celulares de prueba), no manda mensajes viejos.
pub(crate) fn vigencia(plantilla: &PlantillaWhatsApp) -> TimeDelta {
    match plantilla {
        PlantillaWhatsApp::CodigoVerificacion { .. } => TimeDelta::minutes(10),
        _ => TimeDelta::hours(24),
    }
}

/// Un parámetro tal como lo acepta Meta: sin saltos de línea ni tabulaciones ni más de un
/// espacio seguido (error 132018). Si en la plantilla la variable va seguida de un punto, el
/// valor no termina en punto: «a las {{6}}.» con «9:00 a. m.» daría «a. m..».
fn parametro(cuerpo: &str, posicion: usize, valor: &str) -> String {
    let limpio = valor.split_whitespace().collect::<Vec<_>>().join(" ");
    if cuerpo.contains(&format!("{{{{{posicion}}}}}.")) {
        limpio.strip_suffix('.').unwrap_or(&limpio).to_owned()
    } else {
        limpio
    }
}

/// Aviso para la administradora cuando un mensaje de una cita no se pudo entregar
/// (RF-NOT-07). El código de verificación no lo amerita: el cliente lo está esperando en
/// pantalla y puede pedir otro.
pub(crate) fn aviso_de_falla(
    plantilla: &PlantillaWhatsApp,
    celular: &str,
    error: &str,
) -> Option<(String, String)> {
    let (titulo, servicio, fecha, hora) = match plantilla {
        PlantillaWhatsApp::CodigoVerificacion { .. } => return None,
        PlantillaWhatsApp::CitaConfirmada {
            nombre,
            servicio,
            fecha,
            hora,
            ..
        } => (
            format!("No se pudo entregar la confirmación de la cita de {nombre}"),
            servicio,
            fecha,
            hora,
        ),
        PlantillaWhatsApp::RecordatorioCita {
            nombre,
            servicio,
            fecha,
            hora,
            ..
        } => (
            format!("No se pudo entregar el recordatorio de la cita de {nombre}"),
            servicio,
            fecha,
            hora,
        ),
        PlantillaWhatsApp::CitaReprogramada {
            nombre,
            servicio,
            fecha,
            hora,
            ..
        } => (
            format!("No se pudo avisar a {nombre} del cambio de su cita"),
            servicio,
            fecha,
            hora,
        ),
        PlantillaWhatsApp::CitaCancelada {
            nombre,
            servicio,
            fecha,
            hora,
        } => (
            format!("No se pudo avisar a {nombre} de la cancelación de su cita"),
            servicio,
            fecha,
            hora,
        ),
        PlantillaWhatsApp::AvisoReprogramacion {
            nombre,
            servicio,
            fecha,
            hora,
            ..
        } => (
            format!("No se pudo pedir a {nombre} que reprograme su cita"),
            servicio,
            fecha,
            hora,
        ),
    };
    let detalle = format!(
        "{servicio}, el {fecha} a las {hora}. Celular {}. Motivo: {error}. Comunícate con el cliente por otro medio.",
        celular_legible(celular)
    );
    Some((titulo, detalle))
}

/// `3001112233` → `300 111 2233`.
fn celular_legible(celular: &str) -> String {
    if celular.len() == 10 && celular.chars().all(|c| c.is_ascii_digit()) {
        format!("{} {} {}", &celular[..3], &celular[3..6], &celular[6..])
    } else {
        celular.to_owned()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn todas() -> Vec<PlantillaWhatsApp> {
        let t = |s: &str| s.to_owned();
        vec![
            PlantillaWhatsApp::CodigoVerificacion { codigo: t("4821") },
            PlantillaWhatsApp::CitaConfirmada {
                nombre: t("Andrea"),
                servicio: t("Corte clásico"),
                barbero: t("Daniel"),
                fecha: t("lunes 23 de noviembre"),
                hora: t("9:00 a. m."),
                enlace: t("https://diamante.co/cita/abc"),
            },
            PlantillaWhatsApp::RecordatorioCita {
                nombre: t("Andrea"),
                servicio: t("Corte clásico"),
                barbero: t("Daniel"),
                fecha: t("lunes 23 de noviembre"),
                hora: t("9:00 a. m."),
            },
            PlantillaWhatsApp::CitaReprogramada {
                nombre: t("Andrea"),
                servicio: t("Corte clásico"),
                barbero: t("Felipe"),
                fecha: t("martes 24 de noviembre"),
                hora: t("3:30 p. m."),
                enlace: t("https://diamante.co/cita/def"),
            },
            PlantillaWhatsApp::CitaCancelada {
                nombre: t("Andrea"),
                servicio: t("Corte clásico"),
                fecha: t("lunes 23 de noviembre"),
                hora: t("9:00 a. m."),
            },
            PlantillaWhatsApp::AvisoReprogramacion {
                nombre: t("Andrea"),
                servicio: t("Corte clásico"),
                fecha: t("lunes 23 de noviembre"),
                hora: t("9:00 a. m."),
                enlace: t("https://diamante.co/cita/ghi"),
            },
        ]
    }

    #[test]
    fn cada_variante_tiene_su_definicion_con_el_mismo_nombre_que_en_la_cola() {
        for plantilla in todas() {
            let en_cola = serde_json::to_value(&plantilla).unwrap()["plantilla"]
                .as_str()
                .unwrap()
                .to_owned();
            let renderizado = renderizar(&plantilla, "Diamante Barbería");
            assert_eq!(renderizado.plantilla, en_cola);
            assert!(
                !renderizado.texto.contains("{{"),
                "{en_cola}: quedó una variable sin reemplazar"
            );
            let variables = definicion(&en_cola).unwrap().cuerpo.matches("{{").count();
            assert_eq!(variables, renderizado.parametros.len(), "{en_cola}");
        }
    }

    #[test]
    fn ninguna_plantilla_de_utilidad_empieza_ni_termina_con_una_variable() {
        for definicion in DEFINICIONES.iter().filter(|d| d.categoria == "UTILITY") {
            assert!(!definicion.cuerpo.starts_with("{{"), "{}", definicion.nombre);
            assert!(
                !definicion.cuerpo.trim_end().ends_with("}}"),
                "{}",
                definicion.nombre
            );
        }
    }

    #[test]
    fn el_documento_de_plantillas_coincide_con_el_codigo() {
        let documento = include_str!("../../../../../docs/diseno/plantillas-whatsapp.md");
        for definicion in &DEFINICIONES {
            assert!(
                documento.contains(definicion.cuerpo),
                "el documento no tiene el texto vigente de «{}»",
                definicion.nombre
            );
            for boton in definicion.botones {
                assert!(documento.contains(boton.texto) && documento.contains(boton.carga));
            }
        }
    }

    #[test]
    fn la_hora_nunca_queda_seguida_de_un_punto() {
        // Con «9:00 a. m.» un punto después de la variable daría «a. m..».
        for plantilla in todas() {
            let texto = renderizar(&plantilla, "Diamante Barbería").texto;
            assert!(!texto.contains(".."), "{texto}");
        }
    }

    #[test]
    fn renderiza_el_texto_y_los_botones_del_recordatorio() {
        let r = renderizar(&todas()[2], "Diamante Barbería");
        assert_eq!(
            r.texto,
            "Hola Andrea.\nTe recordamos tu cita en Diamante Barbería el lunes 23 de noviembre a las 9:00 a. m. \
             con Daniel.\n¿Nos confirmas tu asistencia?"
        );
        assert_eq!(
            r.parametros,
            [
                "Andrea",
                "Diamante Barbería",
                "lunes 23 de noviembre",
                "9:00 a. m.",
                "Daniel"
            ]
        );
        assert_eq!(
            r.botones.iter().map(|b| b.carga).collect::<Vec<_>>(),
            [CONFIRMAR, CANCELAR]
        );
    }

    #[test]
    fn la_confirmacion_lleva_el_negocio_y_la_hora_sin_doble_punto() {
        let r = renderizar(&todas()[1], "Barbería del Centro");
        assert_eq!(
            r.parametros,
            [
                "Andrea",
                "Barbería del Centro",
                "Corte clásico",
                "Daniel",
                "lunes 23 de noviembre",
                "9:00 a. m",
                "https://diamante.co/cita/abc"
            ],
            "{{6}} va seguida de un punto en la plantilla: la hora pierde el suyo"
        );
        assert_eq!(
            r.texto,
            "Hola Andrea.\nTu cita en Barbería del Centro para el servicio Corte clásico quedó agendada con Daniel \
             el lunes 23 de noviembre a las 9:00 a. m.\n\nPuedes verla, reprogramarla o cancelarla aquí: \
             https://diamante.co/cita/abc\n\n¡Te esperamos!"
        );
        let cancelada = renderizar(&todas()[4], "Diamante Barbería");
        assert_eq!(cancelada.parametros[3], "lunes 23 de noviembre a las 9:00 a. m.");
    }

    #[test]
    fn los_parametros_no_llevan_saltos_ni_espacios_de_mas() {
        let p = PlantillaWhatsApp::CitaCancelada {
            nombre: "Ana\n  María\tRuiz".into(),
            servicio: "Corte     clásico".into(),
            fecha: "lunes".into(),
            hora: "9:00 a. m.".into(),
        };
        let r = renderizar(&p, "  Diamante \n Barbería ");
        assert_eq!(
            r.parametros[..3],
            ["Ana María Ruiz", "Diamante Barbería", "Corte clásico"]
        );
    }

    #[test]
    fn avisa_solo_por_los_mensajes_de_citas() {
        assert_eq!(aviso_de_falla(&todas()[0], "3001112233", "x"), None);
        let (titulo, detalle) = aviso_de_falla(&todas()[1], "3001112233", "sin conexión").unwrap();
        assert_eq!(titulo, "No se pudo entregar la confirmación de la cita de Andrea");
        assert!(
            detalle.contains("300 111 2233") && detalle.contains("sin conexión"),
            "{detalle}"
        );
    }
}
