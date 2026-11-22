#!/usr/bin/env bash
# Proceso del servicio `respaldo` (RNF-BAK-01 a 03):
# - cada DIAMANTE_RESPALDO_CADA_MINUTOS (60): respaldo de la base (RPO ≤ 1 hora);
# - el primero desde las DIAMANTE_RESPALDO_HORA_DIARIA (2 a. m.) es el diario: base y archivos;
# - los domingos, después del diario, la prueba de restauración.
# Un reinicio del contenedor no repite el diario del día (marca en /respaldos/diario).
set -uo pipefail
# shellcheck source=comun.sh
source "$(dirname "$0")/comun.sh"

cada=${DIAMANTE_RESPALDO_CADA_MINUTOS:-60}
hora_diaria=${DIAMANTE_RESPALDO_HORA_DIARIA:-2}
raiz=${DIAMANTE_RESPALDO_DIR:-/respaldos}
mkdir -p "$raiz/horario" "$raiz/diario"
[ -n "${PGPASSWORD:-}" ] || { registrar "ERROR: falta DIAMANTE_RESPALDO_PASSWORD" >&2; exit 78; }
registrar "respaldos cada $cada min; el diario desde las $hora_diaria:00 ($(date +%Z))"

while true; do
  hoy=$(date +%F)
  tipo=horario
  if [ "$((10#$(date +%H)))" -ge "$hora_diaria" ] && [ ! -e "$raiz/diario/.hecho-$hoy" ]; then
    tipo=diario
  fi
  if respaldar.sh "$tipo"; then
    if [ "$tipo" = diario ]; then
      touch "$raiz/diario/.hecho-$hoy"
      if [ "$(date +%u)" = 7 ]; then
        probar-restauracion.sh || registrar "ERROR: la prueba de restauración falló" >&2
      fi
    fi
  else
    registrar "ERROR: el respaldo $tipo falló" >&2
  fi
  sleep "$((cada * 60))"
done
