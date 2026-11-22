# Recorrido de la colección de Postman con newman

```bash
make coleccion             # o pruebas/api/correr-todo.sh
```

Levanta un ambiente E2E nuevo (base en memoria, Mailpit, Gotenberg, WhatsApp simulado y reloj
de pruebas), crea la primera administradora y corre la
[colección](../../docs/pruebas-manuales/diamante-barberia.postman_collection.json) en dos partes:
las carpetas 00 a 20, un reinicio de la API (el límite de 10 inicios de sesión por IP cada 15
minutos) y de la 21 al final. Deja el ambiente arriba para revisarlo:

```bash
docker compose -p diamante-e2e -f docker-compose.yml -f docker-compose.e2e.yml down -v
```

`correr.mjs` es el ejecutor: carpetas por prefijo (`CARPETAS=00,01`), variables de entrada y de
salida (para encadenar corridas) y archivos de muestra para las cargas, que en Postman se eligen
a mano (una foto PNG vertical y un PDF mínimo).

Último recorrido (21/11/2026): 355 peticiones y 683 pruebas, sin fallas, dos veces seguidas.
