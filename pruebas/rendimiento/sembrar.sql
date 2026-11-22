-- Volumen de dos años de una barbería con mucho movimiento, para las pruebas de rendimiento
-- (RNF-REN-02 a 04). Requiere el equipo y los servicios creados por la API (base.mjs).
-- Respeta todas las restricciones y triggers: citas sin solape, ventas que cuadran (triggers
-- diferidos), tiquetes consecutivos y la proyección de métricas de clientes.
--
--   docker compose … exec -T db psql -U diamante_propietario -d diamante < sembrar.sql
\set ON_ERROR_STOP on
\timing on
SELECT setseed(0.42);
BEGIN;

CREATE TEMP TABLE parametros AS
SELECT (SELECT id FROM identidad.usuario WHERE rol = 'administrador' ORDER BY id LIMIT 1) AS admin,
       (now() AT TIME ZONE 'America/Bogota')::date AS hoy,
       (SELECT id FROM ventas.metodo_pago WHERE codigo = 'efectivo') AS efectivo,
       (SELECT id FROM ventas.metodo_pago WHERE codigo = 'nequi') AS nequi,
       (SELECT id FROM ventas.descuento WHERE nombre = '10 %') AS diez,
       (SELECT min(id) FROM ventas.motivo_descuento) AS motivo;

-- Servicios: 1 = el principal (corte), 2 = el segundo, 3 = el complemento (cejas).
CREATE TEMP TABLE servicios AS
SELECT id, precio::bigint AS precio, row_number() OVER (ORDER BY orden, id) AS n
FROM catalogo.servicio WHERE activo AND tipo = 'simple';
CREATE TEMP TABLE equipo AS
SELECT id, row_number() OVER (ORDER BY id) - 1 AS n FROM barberos.barbero WHERE fecha_retiro IS NULL;

-- 1. 5.000 clientes.
INSERT INTO clientes.cliente (celular, nombre, fecha_nacimiento, creado_en)
SELECT '32' || lpad(g::text, 8, '0'), 'Cliente ' || g, date '1965-01-01' + (random() * 16000)::int,
       now() - interval '800 days'
FROM generate_series(1, 5000) g;
CREATE TEMP TABLE clientes_r AS
SELECT id, row_number() OVER (ORDER BY id) AS n FROM clientes.cliente WHERE celular LIKE '32%';

-- 2. Citas: dos años atrás (30 % de las franjas) y un mes adelante (40 %), lunes a sábado,
--    franjas de 30 minutos de 9:00 a 18:30 por barbero.
CREATE TEMP TABLE plan_citas AS
SELECT d::date AS dia, e.id AS barbero_id, s AS franja, random() AS r_ocupa, random() AS r_estado,
       random() AS r_servicio, random() AS r_cliente, random() AS r_origen
FROM parametros p,
     generate_series(p.hoy - 730, p.hoy + 30, interval '1 day') d,
     equipo e,
     generate_series(0, 19) s
WHERE extract(isodow FROM d) < 7;
DELETE FROM plan_citas pc USING parametros p
WHERE (pc.dia < p.hoy AND pc.r_ocupa > 0.30) OR (pc.dia >= p.hoy AND pc.r_ocupa > 0.40);

INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen, creada_por,
                         creada_en, cancelada_en, cancelada_por, motivo_cancelacion, cancelacion_tardia)
SELECT c.id, x.barbero_id, sv.id, x.inicio, x.inicio + interval '30 minutes', sv.precio, x.estado::agenda.estado_cita,
       x.origen::agenda.origen_cita, CASE WHEN x.origen = 'panel' THEN p.admin END,
       x.inicio - interval '2 days',
       CASE WHEN x.estado = 'cancelada' THEN x.inicio - interval '1 day' END,
       CASE WHEN x.estado = 'cancelada' THEN 'cliente'::nucleo.actor END,
       CASE WHEN x.estado = 'cancelada' THEN 'solicitud_cliente'::agenda.motivo_cancelacion END,
       CASE WHEN x.estado = 'cancelada' THEN false END
FROM (
  SELECT pc.*,
         (pc.dia + time '09:00' + pc.franja * interval '30 minutes') AT TIME ZONE 'America/Bogota' AS inicio,
         CASE WHEN pc.dia < (SELECT hoy FROM parametros) THEN
                CASE WHEN pc.r_estado < 0.85 THEN 'atendida' WHEN pc.r_estado < 0.90 THEN 'no_asistio' ELSE 'cancelada' END
              ELSE CASE WHEN pc.r_estado < 0.70 THEN 'sin_confirmar' ELSE 'confirmada' END END AS estado,
         CASE WHEN pc.r_origen < 0.6 THEN 'en_linea' ELSE 'panel' END AS origen,
         CASE WHEN pc.r_servicio < 0.7 THEN 1 WHEN pc.r_servicio < 0.9 THEN 2 ELSE 3 END AS n_servicio,
         1 + floor(pc.r_cliente * 5000)::int AS n_cliente
  FROM plan_citas pc
) x
JOIN servicios sv ON sv.n = x.n_servicio
JOIN clientes_r c ON c.n = x.n_cliente
CROSS JOIN parametros p;

-- 3. Ventas: una por cita atendida (cinco minutos después de terminar) y cinco ocasionales por
--    día. El 30 % suma el complemento, el 10 % lleva 10 % de descuento, propinas variadas y
--    pago en efectivo o Nequi.
CREATE TEMP TABLE plan_ventas AS
SELECT c.id AS cita_id, c.cliente_id, c.barbero_id, c.servicio_id, c.precio::bigint AS precio,
       c.fin + interval '5 minutes' AS registrada_en
FROM agenda.cita c WHERE c.estado = 'atendida'
UNION ALL
SELECT NULL, NULL, e.id, s.id, s.precio,
       (d::date + time '09:00' + ((k * 97 + extract(doy FROM d)::int * 13) % 600) * interval '1 minute')
         AT TIME ZONE 'America/Bogota'
FROM parametros p,
     generate_series(p.hoy - 730, p.hoy - 1, interval '1 day') d,
     generate_series(1, 5) k
JOIN equipo e ON e.n = k % (SELECT count(*) FROM equipo)
JOIN servicios s ON s.n = 1
WHERE extract(isodow FROM d) < 7;

CREATE TEMP TABLE calculo AS
SELECT v.*, row_number() OVER (ORDER BY v.registrada_en, v.cita_id) AS ticket,
       (r_extra < 0.3 AND v.servicio_id <> (SELECT id FROM servicios WHERE n = 3)) AS extra,
       r_desc < 0.1 AS con_descuento,
       CASE WHEN r_propina < 0.6 THEN 0 WHEN r_propina < 0.85 THEN 2000 WHEN r_propina < 0.95 THEN 5000 ELSE 10000 END AS propina,
       CASE WHEN r_metodo < 0.55 THEN (SELECT efectivo FROM parametros) ELSE (SELECT nequi FROM parametros) END AS metodo
FROM (SELECT pv.*, random() AS r_extra, random() AS r_desc, random() AS r_propina, random() AS r_metodo FROM plan_ventas pv) v;
ALTER TABLE calculo ADD COLUMN subtotal bigint, ADD COLUMN descuento bigint, ADD COLUMN total bigint;
UPDATE calculo SET subtotal = precio + CASE WHEN extra THEN (SELECT precio FROM servicios WHERE n = 3) ELSE 0 END;
UPDATE calculo SET descuento = CASE WHEN con_descuento THEN round(subtotal * 0.10)::bigint ELSE 0 END;
UPDATE calculo SET total = subtotal - descuento + propina;

INSERT INTO ventas.venta (numero_ticket, registrada_en, registrada_por, barbero_id, cliente_id, cita_id,
                          descuento_id, motivo_descuento_id, descuento_valor, propina, porcentaje_comision)
SELECT c.ticket, c.registrada_en, p.admin, c.barbero_id, c.cliente_id, c.cita_id,
       CASE WHEN c.descuento > 0 THEN p.diez END, CASE WHEN c.descuento > 0 THEN p.motivo END,
       c.descuento, c.propina, 50
FROM calculo c CROSS JOIN parametros p;
INSERT INTO ventas.venta_linea (venta_id, posicion, servicio_id, precio)
SELECT v.id, 1, c.servicio_id, c.precio FROM calculo c JOIN ventas.venta v ON v.numero_ticket = c.ticket
UNION ALL
SELECT v.id, 2, s.id, s.precio FROM calculo c JOIN ventas.venta v ON v.numero_ticket = c.ticket
JOIN servicios s ON s.n = 3 WHERE c.extra;
INSERT INTO ventas.venta_pago (venta_id, metodo_pago_id, monto)
SELECT v.id, c.metodo, c.total FROM calculo c JOIN ventas.venta v ON v.numero_ticket = c.ticket;
INSERT INTO ventas.contador_ticket (id, ultimo) VALUES (1, (SELECT max(numero_ticket) FROM ventas.venta))
ON CONFLICT (id) DO UPDATE SET ultimo = EXCLUDED.ultimo;

-- 4. Proyección de métricas de clientes (la que mantienen los eventos de ventas y agenda).
INSERT INTO clientes.metricas (cliente_id, visitas, total_gastado, ultima_visita, inasistencias)
SELECT c.id, coalesce(v.visitas, 0), coalesce(v.total, 0), v.ultima, coalesce(i.n, 0)
FROM clientes_r c
LEFT JOIN (
  SELECT cliente_id, count(*) AS visitas, sum(subtotal - descuento) AS total, max((registrada_en AT TIME ZONE 'America/Bogota')::date) AS ultima
  FROM calculo WHERE cliente_id IS NOT NULL GROUP BY cliente_id
) v ON v.cliente_id = c.id
LEFT JOIN (SELECT cliente_id, count(*) AS n FROM agenda.cita WHERE estado = 'no_asistio' GROUP BY cliente_id) i
  ON i.cliente_id = c.id
ON CONFLICT (cliente_id) DO UPDATE
  SET visitas = EXCLUDED.visitas, total_gastado = EXCLUDED.total_gastado,
      ultima_visita = EXCLUDED.ultima_visita, inasistencias = EXCLUDED.inasistencias;
INSERT INTO clientes.visitas_por_barbero (cliente_id, barbero_id, visitas)
SELECT cliente_id, barbero_id, count(*) FROM calculo WHERE cliente_id IS NOT NULL GROUP BY 1, 2
ON CONFLICT (cliente_id, barbero_id) DO UPDATE SET visitas = EXCLUDED.visitas;

COMMIT;
ANALYZE;

SELECT (SELECT count(*) FROM clientes.cliente) AS clientes,
       (SELECT count(*) FROM agenda.cita) AS citas,
       (SELECT count(*) FROM ventas.venta) AS ventas,
       (SELECT count(*) FROM ventas.venta_linea) AS lineas,
       (SELECT pg_size_pretty(pg_database_size(current_database()))) AS tamano;
