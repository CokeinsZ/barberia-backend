# Pruebas de rendimiento (RNF-REN-02 a 04)

```bash
make rendimiento           # o pruebas/rendimiento/correr.sh  (DURACION=20 segundos por escenario)
```

1. Levanta un ambiente aparte (`diamante-rend`, sobre el de E2E, en el puerto 8080) **sin
   límites de uso** (la carga sale de una sola IP y una sola sesión) y con
   `pg_stat_statements`.
2. `base.mjs`: activa la administradora, crea 3 servicios y 6 barberos por la API.
3. `sembrar.sql`: dos años de datos directamente en SQL (5.000 clientes, ~23.600 citas,
   ~22.000 ventas) respetando todas las restricciones y triggers.
4. `carga.mjs`: cada escenario corre `DURACION` segundos con varios clientes a la vez y reporta
   p50, p95 y p99 contra la meta; luego los PDF de un mes. Deja `resultados.json`.
   `FILTRO=dashboard node carga.mjs` corre solo los escenarios que contienen ese texto.
5. Baja el ambiente.

Necesita las imágenes construidas (`VERSION` como en Docker Compose) y Node 18 o superior.
Resultados del 21/11/2026: [docs/operacion/rendimiento.md](../../docs/operacion/rendimiento.md).
