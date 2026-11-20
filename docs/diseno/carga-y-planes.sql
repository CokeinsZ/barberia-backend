\set ON_ERROR_STOP 1
SET client_min_messages = warning;

-- ~5 años de operación: 6.000 clientes y ~60.000 citas atendidas con su venta.
INSERT INTO clientes.cliente (celular, nombre)
SELECT '31' || lpad(g::text, 8, '0'),
       (ARRAY['Andrés','Ricardo','Sara','Daniel','Carlos','Juan Pablo','Laura','Jorge','Felipe','Sebastián'])[1 + g % 10]
       || ' ' || (ARRAY['Mora','Salazar','Ospina','Ocampo','Rendón','Ortiz','Giraldo','Patiño','Soto','Gómez','Peña','Muñoz'])[1 + (g / 10) % 12]
       || ' ' || g
FROM generate_series(1, 6000) g;

-- Citas consecutivas de 45 minutos por barbero, de 9 a. m. a 7 p. m., días hábiles.
WITH ids AS (SELECT array_agg(id ORDER BY id) AS a FROM clientes.cliente WHERE anonimizado_en IS NULL)
INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen)
SELECT ids.a[1 + ((d * 13 + s * 7 + b.id) % array_length(ids.a, 1))],
       b.id,
       (SELECT id FROM catalogo.servicio WHERE nombre = 'Corte de cabello'),
       ts, ts + interval '45 minutes', 35000,
       CASE WHEN ts < '2026-11-18' THEN 'atendida' ELSE 'confirmada' END::agenda.estado_cita,
       'en_linea'
FROM ids, barberos.barbero b
CROSS JOIN generate_series(0, 365 * 5) d
CROSS JOIN generate_series(0, 12) s
CROSS JOIN LATERAL (SELECT ('2022-01-01 09:00-05'::timestamptz + make_interval(days => d) + make_interval(mins => s * 45)) AS ts) t
WHERE extract(isodow FROM ts) < 7
  AND NOT EXISTS (SELECT 1 FROM agenda.cita c WHERE c.barbero_id = b.id AND tstzrange(c.inicio, c.fin) && tstzrange(ts, ts + interval '45 minutes'));

-- Una venta por cita atendida (con el cuadre diferido activo).
BEGIN;
UPDATE ventas.contador_ticket SET ultimo = 1000;
INSERT INTO ventas.venta (numero_ticket, registrada_en, registrada_por, barbero_id, cliente_id, cita_id, propina, porcentaje_comision)
SELECT 1000 + row_number() OVER (ORDER BY c.id), c.fin, (SELECT id FROM identidad.usuario WHERE correo = 'laura@diamante.co'),
       c.barbero_id, c.cliente_id, c.id, 2000, 50
FROM agenda.cita c WHERE c.estado = 'atendida';
INSERT INTO ventas.venta_linea (venta_id, posicion, servicio_id, precio)
SELECT v.id, 1, (SELECT id FROM catalogo.servicio WHERE nombre = 'Corte de cabello'), 35000 FROM ventas.venta v WHERE v.numero_ticket > 1000;
INSERT INTO ventas.venta_pago (venta_id, metodo_pago_id, monto)
SELECT v.id, 1 + (v.id % 5), 37000 FROM ventas.venta v WHERE v.numero_ticket > 1000;
COMMIT;

ANALYZE;

SELECT 'citas', count(*) FROM agenda.cita UNION ALL SELECT 'ventas', count(*) FROM ventas.venta UNION ALL SELECT 'clientes', count(*) FROM clientes.cliente;

\echo '== 1. Agenda del día (todos los barberos)'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT id, barbero_id, inicio, fin, estado FROM agenda.cita
WHERE inicio >= '2026-11-19 00:00-05' AND inicio < '2026-11-20 00:00-05' ORDER BY barbero_id, inicio;

\echo '== 2. Disponibilidad: citas vigentes de un barbero en un mes'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT inicio, fin FROM agenda.cita
WHERE barbero_id = (SELECT min(id) FROM barberos.barbero)
  AND tstzrange(inicio, fin) && tstzrange('2026-11-18 00:00-05', '2026-12-18 00:00-05')
  AND estado NOT IN ('cancelada', 'no_asistio');

\echo '== 3. Búsqueda de clientes por nombre sin tildes'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT id, nombre, celular FROM clientes.cliente
WHERE nucleo.normalizar(nombre) LIKE '%' || nucleo.normalizar('pena 15') || '%' LIMIT 20;

\echo '== 4. Búsqueda de clientes por parte del celular'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT id, nombre, celular FROM clientes.cliente WHERE celular LIKE '%0004512%' LIMIT 20;

\echo '== 5. Caja del día'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT count(*), sum(venta_neta), sum(propina) FROM ventas.v_venta WHERE fecha_operacion = '2026-11-13';

\echo '== 6. Reporte mensual por barbero (liquidación)'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT barbero_id, count(*), sum(venta_neta), sum(comision), sum(propina) FROM ventas.v_venta
WHERE fecha_operacion BETWEEN '2026-10-01' AND '2026-10-31' GROUP BY barbero_id;

\echo '== 7. Recordatorios por enviar en la próxima ventana de 24 h'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT id FROM agenda.cita
WHERE estado IN ('sin_confirmar', 'confirmada')
  AND inicio >= '2026-11-19 09:55-05' AND inicio < '2026-11-19 10:05-05';

\echo '== 8. Historial de un cliente'
EXPLAIN (ANALYZE, COSTS OFF, SUMMARY ON, TIMING OFF)
SELECT id, inicio, estado FROM agenda.cita WHERE cliente_id = (SELECT id FROM clientes.cliente ORDER BY id OFFSET 42 LIMIT 1) ORDER BY inicio DESC LIMIT 10;
