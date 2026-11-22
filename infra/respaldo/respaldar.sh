#!/usr/bin/env bash
# Un respaldo: `respaldar.sh horario` (solo la base, se guarda 48 h) o `respaldar.sh diario`
# (base y archivos, se guarda 30 días). RNF-BAK-01 y 02.
#
# - La base va con pg_dump en formato propio (comprimido, restaurable tabla por tabla) y un
#   manifiesto con las filas de cada tabla tomado en la MISMA instantánea (pg_export_snapshot):
#   la prueba de restauración exige que coincidan exactamente.
# - Cada archivo lleva su suma SHA-256 y el volcado se valida leyéndolo (pg_restore --list).
# - Con DIAMANTE_RESPALDO_REMOTO (un remoto de rclone, p. ej. `cifrado:` sobre un bucket) se
#   copia fuera del servidor y se aplica allá la misma retención.
#
# Conexión: variables PG* (PGHOST, PGDATABASE, PGUSER=diamante_respaldo, PGPASSWORD).
set -euo pipefail
# shellcheck source=comun.sh
source "$(dirname "$0")/comun.sh"

tipo=${1:-horario}
case "$tipo" in horario | diario) ;; *) echo "Uso: respaldar.sh horario|diario" >&2; exit 2 ;; esac
base=${PGDATABASE:?Defina PGDATABASE}
raiz=${DIAMANTE_RESPALDO_DIR:-/respaldos}
destino="$raiz/$tipo"
marca=$(date +%Y%m%d-%H%M)
volcado="$destino/$base-$marca.dump"
mkdir -p "$raiz/horario" "$raiz/diario"

esperar_base

# Sesión que fija la instantánea: el manifiesto y pg_dump ven exactamente los mismos datos.
coproc SESION { psql -X -q -A -t -v ON_ERROR_STOP=1 2>&1; }
printf '%s\n' "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;" "SELECT pg_export_snapshot();" >&"${SESION[1]}"
IFS= read -r instantanea <&"${SESION[0]}"
printf '%s\n' "$CONTEO_SQL" "SELECT '--fin--';" >&"${SESION[1]}"
: > "$volcado.filas.parcial"
while IFS= read -r linea <&"${SESION[0]}"; do
  [ "$linea" = "--fin--" ] && break
  printf '%s\n' "$linea" >> "$volcado.filas.parcial"
done
pg_dump --snapshot="$instantanea" --format=custom --compress=6 --file="$volcado.parcial" "$base"
printf '%s\n' "COMMIT;" '\q' >&"${SESION[1]}"
wait "$SESION_PID" 2>/dev/null || true

pg_restore --list "$volcado.parcial" > /dev/null
mv "$volcado.parcial" "$volcado"
mv "$volcado.filas.parcial" "$volcado.filas"
(cd "$destino" && sha256sum "$(basename "$volcado")" "$(basename "$volcado").filas" > "$(basename "$volcado").sha256")
registrar "respaldo $tipo: $(basename "$volcado") ($(du -h "$volcado" | cut -f1), $(wc -l < "$volcado.filas") tablas, $(awk -F'|' '{s+=$2} END {print s}' "$volcado.filas") filas)"

if [ "$tipo" = diario ]; then
  if [ -d /datos/archivos ]; then
    paquete="$destino/archivos-$marca.tar.gz"
    tar -czf "$paquete.parcial" -C /datos archivos
    mv "$paquete.parcial" "$paquete"
    (cd "$destino" && sha256sum "$(basename "$paquete")" > "$(basename "$paquete").sha256")
    registrar "archivos: $(basename "$paquete") ($(du -h "$paquete" | cut -f1))"
  else
    registrar "AVISO: no está montado /datos/archivos; el respaldo diario no incluye fotos ni documentos" >&2
  fi
fi

# Retención local.
horas=${DIAMANTE_RESPALDO_HORAS_HORARIOS:-48}
dias=${DIAMANTE_RESPALDO_RETENCION_DIAS:-30}
find "$raiz/horario" -type f -mmin +"$((horas * 60))" -delete
find "$raiz/diario" -type f -mtime +"$dias" -delete

# Copia fuera del servidor.
if [ -n "${DIAMANTE_RESPALDO_REMOTO:-}" ]; then
  preparar_rclone
  remoto="${DIAMANTE_RESPALDO_REMOTO%/}"
  rclone copy "$destino" "$remoto/$tipo" --exclude '.hecho-*' --exclude '*.parcial'
  if [ "$tipo" = diario ]; then
    rclone delete "$remoto/diario" --min-age "$((dias + 1))d"
  else
    rclone delete "$remoto/horario" --min-age "${horas}h"
  fi
  registrar "copia externa en $remoto/$tipo"
else
  registrar "AVISO: sin DIAMANTE_RESPALDO_REMOTO no hay copia fuera del servidor (RNF-BAK-01)" >&2
fi
