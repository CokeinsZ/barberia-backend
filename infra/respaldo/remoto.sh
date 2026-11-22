#!/usr/bin/env bash
# rclone con la configuración de los respaldos (archivo montado o DIAMANTE_RESPALDO_RCLONE_CONF_B64):
#   docker compose run --rm --no-deps respaldo remoto.sh ls cifrado:diario
#   docker compose run --rm --no-deps respaldo remoto.sh copy cifrado:diario /respaldos/diario
set -euo pipefail
# shellcheck source=comun.sh
source "$(dirname "$0")/comun.sh"
preparar_rclone
exec rclone "$@"
