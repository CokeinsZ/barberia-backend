#!/usr/bin/env bash
# Recuperación ante un desastre (ver docs/operacion/runbook.md): restaura un respaldo en una
# base NUEVA, la compara con su manifiesto y solo entonces la pone en lugar de la actual, que
# queda como diamante_antes_AAAAMMDDHHMM (se borra a mano cuando todo esté bien).
#
#   docker compose stop api worker
#   docker compose run --rm -e PGUSER=diamante_propietario -e PGPASSWORD="$POSTGRES_PASSWORD" \
#     respaldo restaurar.sh /respaldos/diario/diamante-AAAAMMDD-HHMM.dump
#   docker compose start api worker
#
# Necesita el rol propietario: conserva dueños y permisos (diamante_app) del respaldo.
set -euo pipefail
# shellcheck source=comun.sh
source "$(dirname "$0")/comun.sh"

volcado=${1:?Uso: restaurar.sh /respaldos/diario/diamante-AAAAMMDD-HHMM.dump}
base=${PGDATABASE:-diamante}
nueva="${base}_restaurada"
anterior="${base}_antes_$(date +%Y%m%d%H%M)"
esperar_base
(cd "$(dirname "$volcado")" && sha256sum --quiet -c "$(basename "$volcado").sha256")

registrar "restaurando $(basename "$volcado") en $nueva…"
dropdb --if-exists "$nueva"
createdb "$nueva"
pg_restore --exit-on-error --dbname="$nueva" "$volcado"
restaurado=$(mktemp)
psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$nueva" -c "$CONTEO_SQL" > "$restaurado"
if ! diff -u "$volcado.filas" "$restaurado"; then
  registrar "ERROR: $nueva no coincide con el manifiesto; la base actual NO se tocó" >&2
  exit 1
fi
registrar "verificada: $(wc -l < "$restaurado") tablas idénticas al respaldo"

psql -X -q -v ON_ERROR_STOP=1 -d postgres \
  -v base="$base" -v nueva="$nueva" -v anterior="$anterior" <<'SQL'
SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname IN (:'base', :'nueva') AND pid <> pg_backend_pid();
ALTER DATABASE :"base" RENAME TO :"anterior";
ALTER DATABASE :"nueva" RENAME TO :"base";
SQL
registrar "listo: $base es el respaldo restaurado; la anterior quedó como $anterior"
