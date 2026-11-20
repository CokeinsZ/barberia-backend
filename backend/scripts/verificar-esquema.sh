#!/usr/bin/env bash
# Verifica que las migraciones por módulo producen exactamente el esquema de diseño
# (docs/diseno/esquema.sql), con los mismos datos iniciales, y que el resultado pasa las
# pruebas de reglas de negocio (docs/diseno/pruebas-esquema.sql).
#
# Uso: PRUEBAS_DATABASE_URL=postgres://postgres@127.0.0.1:5432/postgres ./scripts/verificar-esquema.sh
# Requiere psql y pg_dump de la misma versión mayor que el servidor (18).
set -euo pipefail

ADMIN="${PRUEBAS_DATABASE_URL:?Defina PRUEBAS_DATABASE_URL}"
RAIZ="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$(mktemp -d)"
A="verif_diseno_$$"
B="verif_migraciones_$$"
url() { echo "${ADMIN%/*}/$1"; }

limpiar() {
  psql "$ADMIN" -qc "DROP DATABASE IF EXISTS $A WITH (FORCE)" -c "DROP DATABASE IF EXISTS $B WITH (FORCE)" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap limpiar EXIT

# pg_dump agrega líneas \restrict con una clave aleatoria en cada volcado: se descartan.
volcar() { pg_dump --no-owner --no-privileges --exclude-table=public._sqlx_migrations "$@" | grep -v -E '^\\(un)?restrict '; }

psql "$ADMIN" -qc "CREATE DATABASE $A" -c "CREATE DATABASE $B"

echo "1/4 Aplicando el esquema de diseño…"
psql "$(url "$A")" -q -v ON_ERROR_STOP=1 -f "$RAIZ/docs/diseno/esquema.sql"

echo "2/4 Aplicando las migraciones por módulo con 'diamante migrar'…"
(cd "$RAIZ/backend" && DATABASE_URL="$(url "$B")" DIAMANTE_LOGS=texto RUST_LOG=warn cargo run -q -p app -- migrar)

echo "3/4 Comparando estructura y datos iniciales…"
volcar --schema-only "$(url "$A")" >"$TMP/diseno.sql"
volcar --schema-only "$(url "$B")" >"$TMP/migraciones.sql"
# Las marcas de tiempo (creado_en, actualizado_en) dependen del momento de la carga: se enmascaran.
enmascarar() { sed -E 's/[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?[+-][0-9]{2}/<instante>/g'; }
volcar --data-only "$(url "$A")" | enmascarar >"$TMP/diseno-datos.sql"
volcar --data-only "$(url "$B")" | enmascarar >"$TMP/migraciones-datos.sql"
diff -u "$TMP/diseno.sql" "$TMP/migraciones.sql"
diff -u "$TMP/diseno-datos.sql" "$TMP/migraciones-datos.sql"
echo "    Iguales: $(grep -c '^CREATE TABLE' "$TMP/migraciones.sql") tablas, $(grep -c '^CREATE INDEX\|^CREATE UNIQUE INDEX' "$TMP/migraciones.sql") índices explícitos."

echo "4/4 Ejecutando las pruebas de reglas de negocio sobre la base migrada…"
salida="$(psql "$(url "$B")" -q -f "$RAIZ/docs/diseno/pruebas-esquema.sql" 2>&1)"
ok="$(grep -c 'OK  ' <<<"$salida" || true)"
if grep -q 'FALLA' <<<"$salida" || [ "$ok" -ne 27 ]; then
  echo "$salida" | grep -E 'OK|FALLA|ERROR' >&2
  echo "Las pruebas del esquema fallaron ($ok de 27)." >&2
  exit 1
fi
echo "    $ok de 27 pruebas superadas."
echo "El esquema de las migraciones coincide con el diseño."
