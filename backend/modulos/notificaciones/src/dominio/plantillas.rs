//! Contenido de los correos, en texto plano y HTML.

use crate::api::PlantillaCorreo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorreoRenderizado {
    pub(crate) asunto: String,
    pub(crate) texto: String,
    pub(crate) html: String,
}

const MARCA: &str = "Diamante Barbería";

pub(crate) fn renderizar(plantilla: &PlantillaCorreo) -> CorreoRenderizado {
    match plantilla {
        PlantillaCorreo::InvitacionPersonal { nombre, rol, enlace } => armar(
            format!("Tu acceso al panel de {MARCA}"),
            nombre,
            &format!(
                "Te dimos acceso al panel de {MARCA} como {rol}. Crea tu contraseña con este enlace (vence en 7 días):"
            ),
            "Crear mi contraseña",
            enlace,
        ),
        PlantillaCorreo::RecuperacionContrasena { nombre, enlace } => armar(
            format!("Restablece tu contraseña de {MARCA}"),
            nombre,
            "Recibimos una solicitud para restablecer tu contraseña. Usa este enlace (vence en 1 hora). \
             Si no fuiste tú, ignora este correo: tu contraseña no cambia.",
            "Restablecer mi contraseña",
            enlace,
        ),
    }
}

fn armar(asunto: String, nombre: &str, mensaje: &str, boton: &str, enlace: &str) -> CorreoRenderizado {
    let texto = format!("Hola, {nombre}:\n\n{mensaje}\n\n{enlace}\n\n— {MARCA}\n");
    let html = format!(
        "<!doctype html><html lang=\"es\"><body style=\"font-family:Arial,sans-serif;color:#1a1712\">\
         <p>Hola, {nombre}:</p><p>{mensaje}</p>\
         <p><a href=\"{enlace}\" style=\"display:inline-block;padding:12px 20px;background:#c9a35a;color:#15120d;\
         text-decoration:none;font-weight:bold;border-radius:3px\">{boton}</a></p>\
         <p style=\"font-size:12px;color:#6e675b\">Si el botón no funciona, copia este enlace: {enlace}</p>\
         <p>— {MARCA}</p></body></html>",
        nombre = escapar(nombre),
        mensaje = escapar(mensaje),
        boton = escapar(boton),
        enlace = escapar(enlace),
    );
    CorreoRenderizado { asunto, texto, html }
}

fn escapar(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_invitacion_lleva_el_enlace_y_escapa_el_html() {
        let correo = renderizar(&PlantillaCorreo::InvitacionPersonal {
            nombre: "Laura <script>".into(),
            rol: "administradora".into(),
            enlace: "https://diamante.co/acceso/activar?token=abc&x=1".into(),
        });
        assert!(correo.asunto.contains("panel"));
        assert!(
            correo
                .texto
                .contains("https://diamante.co/acceso/activar?token=abc&x=1")
        );
        assert!(correo.html.contains("Laura &lt;script&gt;"));
        assert!(correo.html.contains("token=abc&amp;x=1"));
        assert!(!correo.html.contains("<script>"));
    }
}
