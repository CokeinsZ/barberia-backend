#!/bin/sh
# Se ejecuta una sola vez, al crear el volumen de la base de datos (docker-entrypoint-initdb.d).
# Crea el rol de la aplicación con permisos mínimos (solo DML, sin DDL) y deja configurados
# los privilegios por defecto para que alcancen a los objetos que creen las migraciones.
set -eu

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
  -v base="$POSTGRES_DB" -v propietario="$POSTGRES_USER" -v clave_app="$DIAMANTE_APP_PASSWORD" <<'SQL'
CREATE ROLE diamante_app LOGIN PASSWORD :'clave_app';
GRANT CONNECT ON DATABASE :"base" TO diamante_app;

-- Lo que creen las migraciones (que corren con el rol propietario) queda disponible para la
-- aplicación: uso de esquemas, lectura y escritura de tablas y uso de secuencias.
ALTER DEFAULT PRIVILEGES FOR ROLE :"propietario" GRANT USAGE ON SCHEMAS TO diamante_app;
ALTER DEFAULT PRIVILEGES FOR ROLE :"propietario" GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO diamante_app;
ALTER DEFAULT PRIVILEGES FOR ROLE :"propietario" GRANT USAGE, SELECT ON SEQUENCES TO diamante_app;
SQL
