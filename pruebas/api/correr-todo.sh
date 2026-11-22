#!/usr/bin/env bash
# Recorrido completo de la colección contra un ambiente E2E nuevo (base en memoria):
#   pruebas/api/correr-todo.sh
# Levanta el ambiente, crea la primera administradora, corre las carpetas 00 a 20, reinicia la
# API (límite de 10 inicios de sesión por IP cada 15 minutos) y corre de la 21 al final. Deja
# el ambiente arriba para revisarlo; se baja con:
#   docker compose -p diamante-e2e -f docker-compose.yml -f docker-compose.e2e.yml down -v
set -euo pipefail
cd "$(dirname "$0")/../.."
compose=(docker compose -p diamante-e2e -f docker-compose.yml -f docker-compose.e2e.yml)
salida=${SALIDA:-$(mktemp -d)}
[ -d pruebas/api/node_modules ] || (cd pruebas/api && npm install --no-audit --no-fund)

"${compose[@]}" down -v --remove-orphans >/dev/null 2>&1 || true
"${compose[@]}" up -d --wait
"${compose[@]}" exec -T api /app/diamante crear-administrador laura@diamante.co "Laura Gómez"
hoy=$(TZ=America/Bogota date +%F)
comunes=(secreto_whatsapp=secreto-de-pruebas-whatsapp token_webhook=token-de-pruebas "hoy=$hoy")

CARPETAS=$(seq -s, -f '%02g' 0 20) node pruebas/api/correr.mjs "$salida/parte-a.json" "" "${comunes[@]}"
"${compose[@]}" restart api
"${compose[@]}" up -d --wait
CARPETAS=21,22,23,24,25,26,27,28,29,Utilidades node pruebas/api/correr.mjs "$salida/parte-b.json" "$salida/parte-a.json"
echo "Variables finales en $salida"
