#!/usr/bin/env bash
# Pruebas de rendimiento de punta a punta (RNF-REN-02 a 04): levanta un ambiente aparte
# (proyecto diamante-rend, puerto 8080), crea el equipo por la API, siembra dos años de datos
# (sembrar.sql), corre la carga (carga.mjs) y baja el ambiente. Resultados en resultados.json.
#   pruebas/rendimiento/correr.sh            (DURACION=20 segundos por escenario)
set -euo pipefail
cd "$(dirname "$0")/../.."
r=(docker compose -p diamante-rend -f docker-compose.yml -f docker-compose.e2e.yml -f pruebas/rendimiento/docker-compose.rendimiento.yml)
trap '"${r[@]}" down -v >/dev/null 2>&1 || true' EXIT
"${r[@]}" down -v >/dev/null 2>&1 || true
"${r[@]}" up -d --wait
token=$("${r[@]}" exec -T api /app/diamante crear-administrador laura@diamante.co "Laura Gómez" | grep -o 'token=[A-Za-z0-9_-]*' | cut -d= -f2)
(cd pruebas/rendimiento && TOKEN=$token node base.mjs)
"${r[@]}" exec -T db psql -q -U diamante_propietario -d diamante < pruebas/rendimiento/sembrar.sql
"${r[@]}" exec -T db psql -q -U diamante_propietario -d diamante -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements" >/dev/null
(cd pruebas/rendimiento && node carga.mjs)
