//! Reglas de los archivos: tipos admitidos (detectados por su contenido), claves seguras y
//! nombres originales saneados.

use std::fmt;

/// Tipos admitidos (coinciden con la restricción `CHECK` de `archivos.archivo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoArchivo {
    Jpeg,
    Png,
    Webp,
    Pdf,
}

impl TipoArchivo {
    pub const IMAGENES: &'static [TipoArchivo] = &[TipoArchivo::Jpeg, TipoArchivo::Png, TipoArchivo::Webp];
    pub const DOCUMENTOS: &'static [TipoArchivo] = &[
        TipoArchivo::Pdf,
        TipoArchivo::Jpeg,
        TipoArchivo::Png,
        TipoArchivo::Webp,
    ];

    pub fn mime(self) -> &'static str {
        match self {
            TipoArchivo::Jpeg => "image/jpeg",
            TipoArchivo::Png => "image/png",
            TipoArchivo::Webp => "image/webp",
            TipoArchivo::Pdf => "application/pdf",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            TipoArchivo::Jpeg => "jpg",
            TipoArchivo::Png => "png",
            TipoArchivo::Webp => "webp",
            TipoArchivo::Pdf => "pdf",
        }
    }

    pub fn desde_mime(mime: &str) -> Option<Self> {
        [
            TipoArchivo::Jpeg,
            TipoArchivo::Png,
            TipoArchivo::Webp,
            TipoArchivo::Pdf,
        ]
        .into_iter()
        .find(|t| t.mime() == mime)
    }

    pub fn desde_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(TipoArchivo::Jpeg),
            "png" => Some(TipoArchivo::Png),
            "webp" => Some(TipoArchivo::Webp),
            "pdf" => Some(TipoArchivo::Pdf),
            _ => None,
        }
    }

    pub fn es_imagen(self) -> bool {
        self != TipoArchivo::Pdf
    }

    /// Detecta el tipo por las «firmas mágicas» del contenido: la extensión y el
    /// `Content-Type` que envía el navegador no son confiables.
    pub fn detectar(contenido: &[u8]) -> Option<Self> {
        if contenido.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(TipoArchivo::Jpeg)
        } else if contenido.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(TipoArchivo::Png)
        } else if contenido.len() >= 12 && &contenido[..4] == b"RIFF" && &contenido[8..12] == b"WEBP" {
            Some(TipoArchivo::Webp)
        } else if contenido.starts_with(b"%PDF-") {
            Some(TipoArchivo::Pdf)
        } else {
            None
        }
    }

    pub(crate) fn nombres(tipos: &[TipoArchivo]) -> String {
        let nombres: Vec<&str> = tipos
            .iter()
            .map(|t| match t {
                TipoArchivo::Jpeg => "JPG",
                TipoArchivo::Png => "PNG",
                TipoArchivo::Webp => "WebP",
                TipoArchivo::Pdf => "PDF",
            })
            .collect();
        nombres.join(", ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibilidad {
    Publico,
    Privado,
}

impl Visibilidad {
    /// Valor del enum `archivos.visibilidad` y primera carpeta de la clave.
    pub fn como_str(self) -> &'static str {
        match self {
            Visibilidad::Publico => "publico",
            Visibilidad::Privado => "privado",
        }
    }

    pub fn desde_str(texto: &str) -> Option<Self> {
        match texto {
            "publico" => Some(Visibilidad::Publico),
            "privado" => Some(Visibilidad::Privado),
            _ => None,
        }
    }
}

/// Ruta relativa de un archivo dentro del almacenamiento, p. ej.
/// `publico/barberos/fotos/q3F…8w.jpg`. Solo admite caracteres seguros y nunca `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClaveArchivo(String);

impl ClaveArchivo {
    /// Clave nueva con 128 bits aleatorios.
    pub(crate) fn generar(
        visibilidad: Visibilidad,
        carpeta: &str,
        tipo: TipoArchivo,
        aleatorio: [u8; 16],
    ) -> Option<Self> {
        use base64::Engine;
        let nombre = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(aleatorio);
        Self::validar(&format!(
            "{}/{carpeta}/{nombre}.{}",
            visibilidad.como_str(),
            tipo.extension()
        ))
    }

    /// Acepta solo rutas relativas con letras, números, `-`, `_`, `.` y `/`, sin segmentos
    /// vacíos ni `..`, que empiecen por `publico/` o `privado/`.
    pub fn validar(texto: &str) -> Option<Self> {
        let seguro = !texto.is_empty()
            && texto.len() <= 200
            && (texto.starts_with("publico/") || texto.starts_with("privado/"))
            && texto
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
            && texto.split('/').all(|s| !s.is_empty() && s != "." && s != "..");
        seguro.then(|| Self(texto.to_owned()))
    }

    pub fn como_str(&self) -> &str {
        &self.0
    }

    pub fn es_publica(&self) -> bool {
        self.0.starts_with("publico/")
    }

    /// URL pública (`/media/publico/…`). Solo tiene sentido para claves públicas.
    pub fn url_publica(&self) -> String {
        format!("/media/{}", self.0)
    }
}

impl fmt::Display for ClaveArchivo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Nombre original sin rutas ni caracteres de control, de 1 a 200 caracteres.
pub(crate) fn sanear_nombre(nombre: &str) -> String {
    let base = nombre.rsplit(['/', '\\']).next().unwrap_or_default();
    let limpio: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let limpio = limpio.trim();
    if limpio.is_empty() {
        "archivo".to_owned()
    } else {
        limpio.to_owned()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn detecta_el_tipo_por_el_contenido() {
        assert_eq!(
            TipoArchivo::detectar(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(TipoArchivo::Jpeg)
        );
        assert_eq!(
            TipoArchivo::detectar(b"\x89PNG\r\n\x1a\n...."),
            Some(TipoArchivo::Png)
        );
        assert_eq!(
            TipoArchivo::detectar(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(TipoArchivo::Webp)
        );
        assert_eq!(TipoArchivo::detectar(b"%PDF-1.7"), Some(TipoArchivo::Pdf));
        assert_eq!(TipoArchivo::detectar(b"<html><script>"), None);
        assert_eq!(TipoArchivo::detectar(b""), None);
    }

    #[test]
    fn las_claves_no_permiten_salir_de_la_carpeta() {
        assert!(ClaveArchivo::validar("publico/barberos/fotos/abc_-1.jpg").is_some());
        for mala in [
            "../etc/passwd",
            "publico/../privado/x.pdf",
            "/publico/x.jpg",
            "publico//x.jpg",
            "otro/x.jpg",
            "publico/x y.jpg",
            "publico/./x.jpg",
        ] {
            assert!(ClaveArchivo::validar(mala).is_none(), "{mala}");
        }
        let clave = ClaveArchivo::generar(
            Visibilidad::Privado,
            "barberos/documentos",
            TipoArchivo::Pdf,
            [7; 16],
        )
        .unwrap();
        assert!(
            clave.como_str().starts_with("privado/barberos/documentos/")
                && clave.como_str().ends_with(".pdf")
        );
        assert!(!clave.es_publica());
    }

    #[test]
    fn sanea_el_nombre_original() {
        assert_eq!(sanear_nombre("C:\\Users\\ana\\cédula.pdf"), "cédula.pdf");
        assert_eq!(sanear_nombre("../../x\u{0}.pdf"), "x.pdf");
        assert_eq!(sanear_nombre("   "), "archivo");
    }
}
