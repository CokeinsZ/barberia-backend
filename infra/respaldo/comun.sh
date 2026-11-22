#!/usr/bin/env bash
# Funciones comunes de los scripts de respaldo. Se cargan con `source`.

# Línea de log con fecha (hora de Bogotá, TZ de la imagen).
registrar() {
  printf '%s %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')" "$*"
}

# Filas de cada tabla («esquema.tabla|filas», ordenado). Con el mismo resultado antes y
# después de restaurar se sabe que no se perdió nada. La usan los scripts que cargan este.
# shellcheck disable=SC2034
CONTEO_SQL="SELECT n.nspname || '.' || c.relname || '|' ||
       (xpath('/row/c/text()', query_to_xml(format('SELECT count(*) AS c FROM %I.%I', n.nspname, c.relname), false, true, '')))[1]::text
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'p') AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\\_toast%'
ORDER BY 1;"

# Espera a que la base acepte conexiones (hasta 2 minutos).
esperar_base() {
  for _ in $(seq 1 60); do
    pg_isready -q && return 0
    sleep 2
  done
  registrar "ERROR: la base de datos no responde" >&2
  return 1
}

# Configuración de rclone. Con DIAMANTE_RESPALDO_RCLONE_CONF_B64 (el contenido de rclone.conf
# en base64, para paneles como Dokploy que solo manejan variables) se escribe en un archivo
# privado del contenedor; si no, se usa el montado en /config/rclone/rclone.conf.
preparar_rclone() {
  if [ -n "${DIAMANTE_RESPALDO_RCLONE_CONF_B64:-}" ]; then
    local archivo=/tmp/rclone.conf
    (umask 077 && printf '%s' "$DIAMANTE_RESPALDO_RCLONE_CONF_B64" | base64 -d > "$archivo")
    export RCLONE_CONFIG="$archivo"
  fi
}
