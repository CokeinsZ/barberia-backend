#!/usr/bin/env bash
# Prueba de restauración (RNF-BAK-03): restaura un respaldo (el más reciente si no se indica)
# en una base desechable, comprueba la suma SHA-256 y que cada tabla tenga exactamente las
# filas de su manifiesto, y la borra. Sale con error si algo no coincide.
#
#   probar-restauracion.sh [/respaldos/diario/diamante-AAAAMMDD-HHMM.dump]
#
# Basta el rol diamante_respaldo (CREATEDB): restaura sin dueños ni permisos en una base suya.
set -euo pipefail
# shellcheck source=comun.sh
source "$(dirname "$0")/comun.sh"

raiz=${DIAMANTE_RESPALDO_DIR:-/respaldos}
volcado=${1:-$(find "$raiz/diario" "$raiz/horario" -name '*.dump' -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -n 1 | cut -d' ' -f2-)}
if [ -z "$volcado" ] || [ ! -f "$volcado" ]; then
  echo "No hay respaldos que probar." >&2
  exit 1
fi
esperar_base
(cd "$(dirname "$volcado")" && sha256sum --quiet -c "$(basename "$volcado").sha256")

prueba="diamante_prueba_restauracion"
inicio=$(date +%s)
dropdb --if-exists "$prueba"
createdb "$prueba"
trap 'dropdb --if-exists "$prueba" || true' EXIT
pg_restore --no-owner --no-privileges --no-comments --exit-on-error --dbname="$prueba" "$volcado"

restaurado=$(mktemp)
psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$prueba" -c "$CONTEO_SQL" > "$restaurado"
if ! diff -u "$volcado.filas" "$restaurado"; then
  registrar "ERROR: la restauración de $(basename "$volcado") no coincide con su manifiesto" >&2
  exit 1
fi
registrar "restauración probada: $(basename "$volcado") en $(( $(date +%s) - inicio )) s, $(wc -l < "$restaurado") tablas y $(awk -F'|' '{s+=$2} END {print s}' "$restaurado") filas idénticas"
rm -f "$restaurado"
