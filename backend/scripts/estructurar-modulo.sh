#!/usr/bin/env bash
# Crea la estructura de capas de un módulo (la misma del módulo de referencia `negocio`)
# cuando empieza su implementación. No sobrescribe archivos existentes.
#
# Uso: ./scripts/estructurar-modulo.sh agenda
set -euo pipefail

modulo="${1:?Uso: $0 <modulo>}"
raiz="${MODULOS_DIR:-$(cd "$(dirname "$0")/.." && pwd)/modulos}"
dir="$raiz/$modulo"
[ -f "$dir/Cargo.toml" ] || { echo "No existe el módulo $dir (cree primero su crate)." >&2; exit 1; }

crear() { # crear <ruta> <contenido>
  if [ -e "$dir/$1" ]; then echo "  existe   $1"; else mkdir -p "$(dirname "$dir/$1")"; printf '%s\n' "$2" >"$dir/$1"; echo "  creado   $1"; fi
}

crear src/api.rs "//! Fachada pública del módulo \`$modulo\`: lo único que los demás módulos pueden usar."
crear src/modulo.rs "//! Construcción del módulo: recibe sus dependencias y entrega rutas y fachada."
crear src/dominio/mod.rs "//! Reglas del dominio. Sin SQL, sin HTTP y sin otros módulos.

pub(crate) mod entidades;
pub(crate) mod errores;"
crear src/dominio/entidades/mod.rs "//! Entidades y objetos de valor del módulo."
crear src/dominio/errores.rs "//! Errores de las reglas del dominio."
crear src/aplicacion/mod.rs "//! Casos de uso: un archivo por caso de uso, con su política de autorización.

pub(crate) mod comandos;
pub(crate) mod consultas;
pub(crate) mod dto;
pub(crate) mod errores;
pub(crate) mod politicas;
pub(crate) mod puertos;"
crear src/aplicacion/comandos/mod.rs "//! Comandos: cambian el estado."
crear src/aplicacion/consultas/mod.rs "//! Consultas: solo leen."
crear src/aplicacion/dto.rs "//! Datos que salen de los casos de uso."
crear src/aplicacion/errores.rs "//! Errores de los casos de uso (implementan nucleo::ErrorDeAplicacion)."
crear src/aplicacion/politicas.rs "//! Quién puede ejecutar cada caso de uso."
crear src/aplicacion/puertos/mod.rs "//! Puertos: traits pequeños que la aplicación necesita del exterior."
crear src/infraestructura/mod.rs "//! Implementación de los puertos. SQLx no sale de esta carpeta.

pub(crate) mod postgres;"
crear src/infraestructura/postgres/mod.rs "//! Repositorios de PostgreSQL."
crear src/http/mod.rs "//! Traducción entre HTTP y los casos de uso. Sin reglas de negocio.

mod manejadores;
mod rutas;
mod solicitudes;"
crear src/http/rutas.rs "//! Rutas del módulo, relativas a /api/v1."
crear src/http/manejadores.rs "//! Manejadores HTTP: extraen, llaman al caso de uso y responden."
crear src/http/solicitudes.rs "//! Cuerpos y parámetros de entrada (con #[serde(deny_unknown_fields)])."
crear tests/.gitkeep ""

cat <<MSG

Siguientes pasos para \`$modulo\`:
  1. En src/lib.rs, declare: mod aplicacion; mod dominio; mod http; mod infraestructura; mod modulo; pub mod api;
  2. Agregue en Cargo.toml las dependencias de la plantilla (ver modulos/negocio/Cargo.toml).
  3. Siga el orden del plan: dominio y pruebas → casos de uso con dobles → repositorio PostgreSQL → HTTP.
  4. Registre el módulo en crates/app (composicion.rs y enrutador.rs).
MSG
