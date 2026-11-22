# Informe de rendimiento (RNF-REN-02 a 04)

Medido el 21/11/2026 con `pruebas/rendimiento/correr.sh` (detalle en su
[README](../../pruebas/rendimiento/README.md)): la imagen de producción del backend, PostgreSQL
18 y Gotenberg 8.40 en Docker Compose, con **dos años de una barbería con mucho movimiento**:
6 barberos, 5.000 clientes, 23.649 citas (30 % de las franjas ocupadas, 40 % el próximo mes) y
22.163 ventas con 28.189 líneas (56 MB). Cada escenario corre 20 s con varios clientes a la vez;
la latencia se mide desde el cliente, así que incluye la red local (cota superior del tiempo
en el servidor). Equipo: 16 núcleos y 14 GB de RAM; en un VPS de 2 núcleos los tiempos
subirán, pero los márgenes son de 10 a 100 veces.

## Resultados

| Escenario | Requisito | Concurrencia | p50 | p95 | p99 | Peticiones/s | Meta p95 |
|---|---|---|---|---|---|---|---|
| Disponibilidad del mes, cualquier barbero | RNF-REN-02 | 10 | 8 ms | 10 ms | 12 ms | 1.183 | 500 ms |
| Disponibilidad del mes, un barbero | RNF-REN-02 | 10 | 4 ms | 5 ms | 8 ms | 2.667 | 500 ms |
| Horas libres de un día, cualquier barbero | RNF-REN-02 | 10 | 7 ms | 8 ms | 11 ms | 1.430 | 500 ms |
| Landing | RNF-REN-03 | 10 | 2 ms | 3 ms | 5 ms | 4.080 | 300 ms |
| Agenda del día (listado) | RNF-REN-03 | 4 | 5 ms | 6 ms | 7 ms | 821 | 300 ms |
| Tablero de la agenda | RNF-REN-03 | 4 | 5 ms | 6 ms | 7 ms | 792 | 300 ms |
| Clientes: listado con búsqueda | RNF-REN-03 | 4 | 19 ms | 27 ms | 30 ms | 196 | 300 ms |
| Clientes: búsqueda rápida | RNF-REN-03 | 4 | 15 ms | 45 ms | 47 ms | 165 | 300 ms |
| Caja del día | RNF-REN-03 | 4 | 5 ms | 6 ms | 7 ms | 851 | 300 ms |
| Ventas del día | RNF-REN-03 | 4 | 2 ms | 3 ms | 4 ms | 1.687 | 300 ms |
| Dashboard de ayer | RNF-REN-03 | 4 | 5 ms | 6 ms | 7 ms | 812 | 300 ms |
| Dashboard de hoy | RNF-REN-03 | 4 | 5 ms | 6 ms | 6 ms | 816 | 300 ms |
| Guardar una venta | RNF-REN-03 | 4 | 5 ms | 6 ms | 8 ms | 815 | 300 ms |
| Resumen del mes (vista previa) | — | 2 | 14 ms | 16 ms | 18 ms | 143 | 1 s |
| Clientes del mes (vista previa) | — | 2 | 35 ms | 37 ms | 39 ms | 57 | 1 s |

| PDF (uno a la vez) | Requisito | Tiempo | Tamaño | Meta |
|---|---|---|---|---|
| Resumen del mes | RNF-REN-04 | 0,40 s | 32 KB | 10 s |
| Reporte de un barbero (mes) | RNF-REN-04 | 0,26 s | 51 KB | 10 s |
| Reporte de clientes (mes) | RNF-REN-04 | 0,24 s | 37 KB | 10 s |
| Todos los barberos (mes) | RNF-REN-04 | 0,77 s | 201 KB | 30 s |

Con un volumen extremo (6 barberos × 300 ventas en un mes, 84 páginas), el PDF de todos los
barberos tarda entre 1,3 s (equipo libre) y 12,4 s (equipo compilando a la vez), y el de un
barbero entre 0,4 y 3,6 s (prueba de `infraestructura/gotenberg.rs`): siempre bajo la meta.

## Observaciones

- **Todo cumple.** La operación más pesada del panel es la búsqueda rápida de clientes (45 ms
  p95): busca por nombre sin tildes con trigramas y por celular.
- **Dashboard con un día extremo**: en una primera corrida, el escenario de escritura dejó
  16.808 ventas en el día de hoy antes de medir el dashboard; aun así respondió en 145 ms p95.
  Desde entonces la escritura va al final para no inflar las lecturas del día.
- `pg_stat_statements` (activo en el ambiente de rendimiento) muestra que las consultas usan
  los índices parciales de ventas por fecha y la llave de las líneas; ninguna recorre tablas
  completas en las operaciones del día.
- La disponibilidad no usa caché: se calcula en cada petición con el motor puro sobre las
  citas, bloqueos y ausencias del rango.
