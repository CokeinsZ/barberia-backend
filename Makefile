# Atajos del monorepo. Lee .env si existe.
-include .env

PRUEBAS_DATABASE_URL ?= postgres://diamante_propietario:$(POSTGRES_PASSWORD)@127.0.0.1:5432/postgres

.PHONY: ayuda dev abajo logs migrar administrador imagen backend-formato backend-lint backend-pruebas verificar-esquema \
	rol-respaldo respaldo probar-restauracion coleccion rendimiento

ayuda: ## Muestra esta ayuda
	@grep -hE '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-20s %s\n", $$1, $$2}'

dev: ## Levanta base de datos, migraciones, API y worker en modo desarrollo
	docker compose up -d --build --wait

abajo: ## Detiene los contenedores (conserva los datos)
	docker compose down

logs: ## Sigue los logs de la API y el worker
	docker compose logs -f api worker

migrar: ## Aplica las migraciones pendientes
	docker compose run --rm migraciones

administrador: ## Invita a un administrador: make administrador CORREO=… NOMBRE="…"
	@test -n "$(CORREO)" -a -n "$(NOMBRE)" || (echo 'Uso: make administrador CORREO=laura@diamante.co NOMBRE="Laura Gómez"' && exit 2)
	docker compose run --rm api crear-administrador "$(CORREO)" "$(NOMBRE)"

imagen: ## Construye la imagen del backend
	docker compose build migraciones

backend-formato: ## Formatea el código del backend
	cd backend && cargo fmt --all

backend-lint: ## Verifica el formato y clippy sin advertencias
	cd backend && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings

backend-pruebas: ## Pruebas unitarias y de integración (necesita la base de datos de `make dev`)
	cd backend && PRUEBAS_DATABASE_URL=$(PRUEBAS_DATABASE_URL) cargo test --workspace

verificar-esquema: ## Compara las migraciones con docs/diseno/esquema.sql y corre las 27 pruebas
	cd backend && PRUEBAS_DATABASE_URL=$(PRUEBAS_DATABASE_URL) ./scripts/verificar-esquema.sh

rol-respaldo: ## Crea el rol diamante_respaldo en una base ya existente (usa DIAMANTE_RESPALDO_PASSWORD)
	@test -n "$(DIAMANTE_RESPALDO_PASSWORD)" || (echo 'Defina DIAMANTE_RESPALDO_PASSWORD en .env' && exit 2)
	@printf '%s\n' "CREATE ROLE diamante_respaldo LOGIN CREATEDB PASSWORD :'clave' IN ROLE pg_read_all_data;" \
	  "GRANT CONNECT ON DATABASE diamante TO diamante_respaldo;" \
	  | docker compose exec -T db psql -v ON_ERROR_STOP=1 -U diamante_propietario -d diamante \
	    -v clave="$(DIAMANTE_RESPALDO_PASSWORD)"

respaldo: ## Respaldo inmediato (base y archivos) en el volumen de respaldos
	docker compose --profile respaldo run --rm --no-deps respaldo respaldar.sh diario

probar-restauracion: ## Restaura el respaldo más reciente en una base desechable y lo compara
	docker compose --profile respaldo run --rm --no-deps respaldo probar-restauracion.sh

coleccion: ## Recorrido completo de la colección de Postman contra un ambiente E2E nuevo (newman)
	pruebas/api/correr-todo.sh

rendimiento: ## Pruebas de carga con dos años de datos (ver pruebas/rendimiento/README.md)
	pruebas/rendimiento/correr.sh

