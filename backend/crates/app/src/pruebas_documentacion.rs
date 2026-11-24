//! La documentación de la API (`docs/diseno/api.md`) no se aleja del código.
//!
//! La versión 1.0 del documento se escribió antes de programar y, al implementar, el código
//! cambió sin que el documento lo siguiera (la respuesta de `POST /ventas`, `GET /yo`, los
//! códigos de error, rutas opcionales que no se hicieron). Estas pruebas lo impiden:
//!
//! - cada ruta que declara el código (método y plantilla) está en una tabla del documento;
//! - cada ruta de las tablas existe en el código, salvo las marcadas *Pendiente* o *Descartado*
//!   (y una marcada así que ya exista también falla: hay que quitarle la marca);
//! - la tabla de errores (sección 1.3) tiene exactamente los códigos que puede responder el código.
//!
//! Se leen los fuentes como texto (como `pruebas_autorizacion::rutas_del_codigo`): `.route("…",
//! get(…).post(…))` en `modulos/*/src/http/` y en `app`, y los códigos en `fn codigo(&self)` de
//! cada error y en cada `StatusCode::…, "codigo"`. Lo que va después de `#[cfg(test)]` no cuenta.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DOCUMENTO: &str = include_str!("../../../../docs/diseno/api.md");
const METODOS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// El texto de un fuente sin sus pruebas (los módulos `#[cfg(test)]` van al final).
fn fuente(ruta: &Path) -> String {
    let texto = std::fs::read_to_string(ruta).unwrap();
    match texto.find("#[cfg(test)]") {
        Some(i) => texto[..i].to_owned(),
        None => texto,
    }
}

fn archivos_rs(directorio: &Path, salida: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(directorio) else {
        return;
    };
    for entrada in entradas {
        let ruta = entrada.unwrap().path();
        if ruta.is_dir() {
            archivos_rs(&ruta, salida);
        } else if ruta.extension().is_some_and(|e| e == "rs") {
            salida.push(ruta);
        }
    }
}

/// Fuentes que declaran rutas: el `http` de cada módulo y las rutas propias de `app`.
fn fuentes_con_rutas() -> Vec<PathBuf> {
    let raiz = raiz();
    let mut archivos: Vec<PathBuf> = ["enrutador.rs", "sistema.rs", "pruebas_e2e.rs"]
        .iter()
        .map(|a| raiz.join("crates/app/src").join(a))
        .collect();
    for modulo in std::fs::read_dir(raiz.join("modulos")).unwrap() {
        archivos_rs(&modulo.unwrap().path().join("src/http"), &mut archivos);
    }
    archivos
}

/// Normaliza una plantilla de ruta: sin `/api/v1` y con `{}` en cada parámetro (`{*}` si es comodín).
fn normalizar(ruta: &str) -> String {
    let ruta = ruta.split('?').next().unwrap_or(ruta);
    let ruta = ruta.strip_prefix("/api/v1").unwrap_or(ruta);
    let mut salida = String::new();
    let mut resto = ruta;
    while let Some(i) = resto.find('{') {
        salida.push_str(&resto[..i]);
        let fin = resto[i..].find('}').map_or(resto.len(), |f| i + f + 1);
        salida.push_str(if resto[i..].starts_with("{*") { "{*}" } else { "{}" });
        resto = &resto[fin..];
    }
    salida.push_str(resto);
    salida
}

/// `(MÉTODO, ruta normalizada)` de cada `.route("…", get(…).post(…))` del código.
fn rutas_del_codigo() -> BTreeSet<(String, String)> {
    let mut rutas = BTreeSet::new();
    for archivo in fuentes_con_rutas() {
        let texto = fuente(&archivo);
        for (i, _) in texto.match_indices(".route(") {
            let argumentos = &texto[i + ".route(".len()..];
            let Some(literal) = argumentos.trim_start().strip_prefix('"') else {
                continue;
            };
            let ruta = &literal[..literal.find('"').unwrap()];
            // El segundo argumento: hasta el paréntesis que cierra `.route(`.
            let mut profundidad = 1;
            let fin = argumentos
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '(' => profundidad += 1,
                        ')' => profundidad -= 1,
                        _ => {}
                    }
                    profundidad == 0
                })
                .map_or(argumentos.len(), |(f, _)| f);
            let manejadores = &argumentos[..fin];
            for metodo in METODOS {
                let patron = format!("{metodo}(");
                let declarado = manejadores.match_indices(&patron).any(|(j, _)| {
                    j == 0 || !manejadores[..j].ends_with(|c: char| c.is_alphanumeric() || c == '_')
                });
                if declarado {
                    rutas.insert((metodo.to_uppercase(), normalizar(ruta)));
                }
            }
        }
    }
    rutas
}

/// Una ruta de una tabla del documento.
#[derive(Debug)]
struct RutaDocumentada {
    metodo: String,
    ruta: String,
    pendiente: bool,
}

/// Las filas `| GET / PUT | `/ruta` … |` de las tablas del documento (una fila puede traer
/// varios métodos y varias rutas).
fn rutas_del_documento() -> Vec<RutaDocumentada> {
    let mut rutas = Vec::new();
    for linea in DOCUMENTO.lines() {
        let celdas: Vec<&str> = linea.split('|').map(str::trim).collect();
        if celdas.len() < 4 || !celdas[0].is_empty() {
            continue;
        }
        let metodos: Vec<&str> = celdas[1].split('/').map(str::trim).collect();
        if metodos.is_empty()
            || !metodos
                .iter()
                .all(|m| METODOS.contains(&m.to_lowercase().as_str()))
        {
            continue;
        }
        // La marca explícita (`*Pendiente (Could).*`, `(pendiente: Could)`, `**Descartado**`), no
        // la palabra suelta: «pendientes-de-cierre» o «mensajes pendientes» no la cuentan.
        let pendiente = ["*Pendiente", "(pendiente:", "Descartado"]
            .iter()
            .any(|p| linea.contains(p));
        for (k, trozo) in celdas[2].split('`').enumerate() {
            if k % 2 == 1 && trozo.starts_with('/') {
                for metodo in &metodos {
                    rutas.push(RutaDocumentada {
                        metodo: metodo.to_uppercase(),
                        ruta: normalizar(trozo),
                        pendiente,
                    });
                }
            }
        }
    }
    rutas
}

/// ¿La plantilla del código atiende la ruta del documento? Un `{}` del código acepta `{}.pdf`
/// (`/reportes/barberos/{archivo}` sirve `{id}` y `{id}.pdf`) y `{*}` acepta el resto de la ruta.
fn atiende(codigo: &str, documento: &str) -> bool {
    let mut c = codigo.split('/');
    let mut d = documento.split('/');
    loop {
        match (c.next(), d.next()) {
            (None, None) => return true,
            (Some("{*}"), Some(_)) => return true,
            (Some(a), Some(b)) if a == b || (a == "{}" && b.starts_with("{}")) => {}
            _ => return false,
        }
    }
}

#[test]
fn cada_ruta_del_codigo_esta_en_el_documento_y_viceversa() {
    let codigo = rutas_del_codigo();
    let documento = rutas_del_documento();
    assert!(
        codigo.len() > 100,
        "no se leyeron las rutas del código ({})",
        codigo.len()
    );
    assert!(
        documento.len() > 100,
        "no se leyeron las tablas del documento ({})",
        documento.len()
    );

    let sin_documentar: Vec<String> = codigo
        .iter()
        .filter(|(m, r)| !documento.iter().any(|d| &d.metodo == m && atiende(r, &d.ruta)))
        .map(|(m, r)| format!("{m} {r}"))
        .collect();
    let inexistentes: Vec<String> = documento
        .iter()
        .filter(|d| !d.pendiente && !codigo.iter().any(|(m, r)| m == &d.metodo && atiende(r, &d.ruta)))
        .map(|d| format!("{} {}", d.metodo, d.ruta))
        .collect();
    let ya_hechas: Vec<String> = documento
        .iter()
        .filter(|d| d.pendiente && codigo.iter().any(|(m, r)| m == &d.metodo && atiende(r, &d.ruta)))
        .map(|d| format!("{} {}", d.metodo, d.ruta))
        .collect();
    assert!(
        sin_documentar.is_empty() && inexistentes.is_empty() && ya_hechas.is_empty(),
        "docs/diseno/api.md no coincide con el código.\n\
         Rutas del código que no están en el documento: {sin_documentar:#?}\n\
         Rutas del documento que no existen (márquelas *Pendiente* o quítelas): {inexistentes:#?}\n\
         Rutas marcadas *Pendiente* que ya existen (quite la marca): {ya_hechas:#?}"
    );
}

fn parece_codigo(texto: &str) -> bool {
    !texto.is_empty()
        && texto.starts_with(|c: char| c.is_ascii_lowercase())
        && texto.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// Literales `"…"` de un trozo de código.
fn literales(texto: &str) -> impl Iterator<Item = &str> {
    texto.split('"').skip(1).step_by(2)
}

/// Los códigos que puede responder el backend: los de `fn codigo(&self) -> &'static str` de cada
/// error y los de cada `ErrorHttp::nuevo(StatusCode::…, "codigo", …)`.
fn codigos_del_codigo() -> BTreeSet<String> {
    let raiz = raiz();
    let mut archivos = Vec::new();
    archivos_rs(&raiz.join("crates"), &mut archivos);
    archivos_rs(&raiz.join("modulos"), &mut archivos);
    let mut codigos = BTreeSet::new();
    for archivo in archivos {
        let nombre = archivo.file_name().unwrap().to_string_lossy();
        if nombre.starts_with("pruebas") && nombre != "pruebas_e2e.rs" {
            continue;
        }
        let texto = fuente(&archivo);
        for (i, _) in texto.match_indices("fn codigo(&self) -> &'static str {") {
            let cuerpo = &texto[i..];
            let mut profundidad = 0;
            let fin = cuerpo
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '{' => profundidad += 1,
                        '}' => profundidad -= 1,
                        _ => {}
                    }
                    c == '}' && profundidad == 0
                })
                .map_or(cuerpo.len(), |(f, _)| f);
            codigos.extend(
                literales(&cuerpo[..fin])
                    .filter(|l| parece_codigo(l))
                    .map(str::to_owned),
            );
        }
        for (i, _) in texto.match_indices("StatusCode::") {
            let resto = &texto[i + "StatusCode::".len()..];
            let resto = resto.trim_start_matches(|c: char| c.is_ascii_uppercase() || c == '_');
            if let Some(resto) = resto.trim_start().strip_prefix(',')
                && let Some(literal) = resto.trim_start().strip_prefix('"')
            {
                let candidato = &literal[..literal.find('"').unwrap_or(0)];
                if parece_codigo(candidato) {
                    codigos.insert(candidato.to_owned());
                }
            }
        }
    }
    codigos
}

/// Los códigos de la tabla de la sección 1.3 (`| 409 | `conflicto_horario` | … |`).
fn codigos_del_documento() -> BTreeSet<String> {
    let inicio = DOCUMENTO.find("### 1.3").expect("falta la sección 1.3");
    let fin = inicio + DOCUMENTO[inicio..].find("### 1.4").expect("falta la sección 1.4");
    let mut codigos = BTreeSet::new();
    for linea in DOCUMENTO[inicio..fin].lines() {
        let celdas: Vec<&str> = linea.split('|').map(str::trim).collect();
        if celdas.len() >= 4 && celdas[1].starts_with(|c: char| c.is_ascii_digit()) {
            codigos.extend(
                literales(&celdas[2].replace('`', "\""))
                    .filter(|l| parece_codigo(l))
                    .map(str::to_owned),
            );
        }
    }
    codigos
}

#[test]
fn la_tabla_de_errores_tiene_los_codigos_del_codigo() {
    let codigo = codigos_del_codigo();
    let documento = codigos_del_documento();
    assert!(
        codigo.len() > 20,
        "no se leyeron los códigos del código: {codigo:?}"
    );
    let faltan: Vec<&String> = codigo.difference(&documento).collect();
    let sobran: Vec<&String> = documento.difference(&codigo).collect();
    assert!(
        faltan.is_empty() && sobran.is_empty(),
        "la tabla de errores de docs/diseno/api.md (1.3) no coincide con el código.\n\
         Códigos que responde el código y no están en la tabla: {faltan:?}\n\
         Códigos de la tabla que ningún error responde: {sobran:?}"
    );
}

#[test]
fn normaliza_y_compara_plantillas() {
    assert_eq!(
        normalizar("/api/v1/barberos/{id}/ausencias/{aid}?x=1"),
        "/barberos/{}/ausencias/{}"
    );
    assert_eq!(normalizar("/media/{*ruta}"), "/media/{*}");
    assert!(atiende("/reportes/barberos/{}", "/reportes/barberos/{}.pdf"));
    assert!(atiende("/media/{*}", "/media/publico/{}"));
    assert!(!atiende("/barberos/{}", "/barberos/{}/horario"));
    assert!(!atiende("/barberos/orden", "/barberos/{}"));
}
