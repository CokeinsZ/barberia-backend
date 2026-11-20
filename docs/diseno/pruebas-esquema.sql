\set ON_ERROR_STOP 1
SET client_min_messages = notice;

-- Utilidad: ejecuta SQL y verifica que falle con el SQLSTATE esperado.
CREATE OR REPLACE FUNCTION pg_temp.espera_error(p_sql text, p_estado text, p_nombre text) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  BEGIN
    EXECUTE p_sql;
  EXCEPTION WHEN others THEN
    IF SQLSTATE = p_estado THEN
      RAISE NOTICE 'OK   %', p_nombre;
      RETURN;
    END IF;
    RAISE EXCEPTION 'FALLA % → SQLSTATE % (esperado %): %', p_nombre, SQLSTATE, p_estado, SQLERRM;
  END;
  RAISE EXCEPTION 'FALLA % → no falló', p_nombre;
END $$;

-- ---------- Datos base ----------
INSERT INTO identidad.usuario (correo, rol) VALUES ('laura@diamante.co', 'administrador'), ('santiago@correo.com', 'barbero'), ('mateo@correo.com', 'barbero');
INSERT INTO identidad.administrador (usuario_id, nombre) SELECT id, 'Laura Martínez' FROM identidad.usuario WHERE correo = 'laura@diamante.co';
INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos, fecha_nacimiento, celular, especialidad, fecha_ingreso, tipo_vinculacion, porcentaje_comision, orden)
SELECT id, 'CC', '1053774210', 'Santiago', 'Ríos', '1996-08-09', '3105550182', 'Fades y degradados', '2022-03-14', 'prestacion_servicios', 50, 1 FROM identidad.usuario WHERE correo = 'santiago@correo.com';
INSERT INTO barberos.barbero (usuario_id, tipo_documento, numero_documento, nombres, apellidos, fecha_nacimiento, celular, especialidad, fecha_ingreso, tipo_vinculacion, porcentaje_comision, orden)
SELECT id, 'CC', '1017228904', 'Mateo', 'Valencia', '1990-01-22', '3005557731', 'Barba y afeitado', '2019-08-02', 'prestacion_servicios', 45, 2 FROM identidad.usuario WHERE correo = 'mateo@correo.com';

INSERT INTO catalogo.servicio (categoria_id, nombre, descripcion, precio, duracion_min) VALUES
  (1, 'Corte de cabello', 'Corte a tijera y máquina', 35000, 45),
  (2, 'Perfilado y diseño de barba', 'Perfilado a navaja', 25000, 30),
  (4, 'Diseño de cejas', 'Cejas a navaja', 12000, 15);
INSERT INTO catalogo.servicio (tipo, categoria_id, nombre, descripcion, precio, duracion_min, destacado, etiqueta)
  VALUES ('combo', 3, 'Combo cabello + barba', 'Corte, barba y toalla caliente', 55000, 75, true, 'Ahorras $5.000');
INSERT INTO catalogo.componente_combo (combo_id, servicio_id)
  SELECT c.id, s.id FROM catalogo.servicio c, catalogo.servicio s
  WHERE c.nombre = 'Combo cabello + barba' AND s.nombre IN ('Corte de cabello', 'Perfilado y diseño de barba');

INSERT INTO clientes.cliente (celular, nombre, correo) VALUES ('3105559917', 'Andrés Felipe Mora', 'andresf.mora@correo.com'), ('3115550958', 'Ricardo Salazar', NULL);

-- ---------- Integridad de subtipos y combos ----------
SELECT pg_temp.espera_error(
  $q$INSERT INTO identidad.administrador (usuario_id, nombre) SELECT id, 'Xavier Prueba' FROM identidad.usuario WHERE correo = 'mateo@correo.com'$q$,
  '23503', 'un administrador no puede apuntar a una cuenta con rol barbero');
SELECT pg_temp.espera_error(
  $q$INSERT INTO catalogo.componente_combo (combo_id, servicio_id) SELECT c.id, c.id + 0 FROM catalogo.servicio c WHERE c.tipo = 'combo'$q$,
  '23503', 'un combo no se contiene a sí mismo (la llave compuesta exige un componente simple)');
INSERT INTO catalogo.servicio (tipo, categoria_id, nombre, descripcion, precio, duracion_min) VALUES ('combo', 3, 'Combo + cejas', 'Combo y cejas', 67000, 90);
SELECT pg_temp.espera_error(
  $q$INSERT INTO catalogo.componente_combo (combo_id, servicio_id) SELECT a.id, b.id FROM catalogo.servicio a, catalogo.servicio b WHERE a.nombre = 'Combo + cejas' AND b.nombre = 'Combo cabello + barba'$q$,
  '23503', 'un combo solo agrupa servicios simples');
SELECT pg_temp.espera_error(
  $q$INSERT INTO catalogo.servicio (categoria_id, nombre, descripcion, precio, duracion_min) VALUES (1, 'CORTE DE CABÉLLO', 'dup', 1000, 30)$q$,
  '23505', 'nombre de servicio único sin importar mayúsculas ni tildes');
SELECT pg_temp.espera_error(
  $q$INSERT INTO identidad.usuario (correo, rol) VALUES ('LAURA@Diamante.co', 'administrador')$q$,
  '23505', 'correo de usuario único sin importar mayúsculas');
SELECT pg_temp.espera_error(
  $q$INSERT INTO clientes.cliente (celular, nombre) VALUES ('2105559917', 'Celular inválido')$q$,
  '23514', 'celular colombiano válido');

-- ---------- Horarios, ausencias y bloqueos ----------
INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin)
  SELECT b.id, d, '09:00', '13:00' FROM barberos.barbero b, generate_series(1, 6) d WHERE b.nombres = 'Santiago';
INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin)
  SELECT b.id, d, '14:00', '19:00' FROM barberos.barbero b, generate_series(1, 5) d WHERE b.nombres = 'Santiago';
INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin)
  SELECT b.id, 7, '10:00', '15:00' FROM barberos.barbero b WHERE b.nombres = 'Mateo';
INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin)
  SELECT b.id, 2, '10:00', '20:00' FROM barberos.barbero b WHERE b.nombres = 'Mateo';
SELECT pg_temp.espera_error(
  $q$INSERT INTO barberos.franja_horaria (barbero_id, dia_semana, hora_inicio, hora_fin) SELECT id, 1, '12:00', '15:00' FROM barberos.barbero WHERE nombres = 'Santiago'$q$,
  '23P01', 'las franjas de un barbero no se solapan en el mismo día');

INSERT INTO barberos.ausencia (barbero_id, tipo, desde, hasta, registrada_por)
  SELECT b.id, 'vacaciones', '2026-11-16', '2026-11-30', u.id FROM barberos.barbero b, identidad.usuario u WHERE b.nombres = 'Mateo' AND u.correo = 'laura@diamante.co';
SELECT pg_temp.espera_error(
  $q$INSERT INTO barberos.ausencia (barbero_id, tipo, desde, hasta, registrada_por) SELECT b.id, 'incapacidad', '2026-11-30', '2026-12-02', u.id FROM barberos.barbero b, identidad.usuario u WHERE b.nombres = 'Mateo' AND u.correo = 'laura@diamante.co'$q$,
  '23P01', 'las ausencias de un barbero no se solapan (los extremos se incluyen)');

SELECT pg_temp.espera_error(
  $q$INSERT INTO negocio.dia_especial (fecha, nombre, tipo) VALUES ('2026-12-24', 'Nochebuena', 'cierre_temprano')$q$,
  '23514', 'un cierre temprano exige hora de cierre');
INSERT INTO negocio.dia_especial (fecha, nombre, tipo, hora_cierre) VALUES ('2026-12-24', 'Nochebuena', 'cierre_temprano', '14:00');

-- ---------- Citas ----------
INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen)
SELECT c.id, b.id, s.id, '2026-11-19 15:30-05', '2026-11-19 16:45-05', s.precio, 'confirmada', 'en_linea'
FROM clientes.cliente c, barberos.barbero b, catalogo.servicio s
WHERE c.celular = '3105559917' AND b.nombres = 'Santiago' AND s.nombre = 'Combo cabello + barba';

SELECT pg_temp.espera_error(
  $q$INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen)
     SELECT c.id, b.id, s.id, '2026-11-19 16:00-05', '2026-11-19 16:45-05', s.precio, 'confirmada', 'en_linea'
     FROM clientes.cliente c, barberos.barbero b, catalogo.servicio s
     WHERE c.celular = '3115550958' AND b.nombres = 'Santiago' AND s.nombre = 'Corte de cabello'$q$,
  '23P01', 'dos citas vigentes del mismo barbero no se cruzan');

-- Una cita que empieza justo cuando termina la anterior sí se permite (rango semiabierto).
INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen)
SELECT c.id, b.id, s.id, '2026-11-19 16:45-05', '2026-11-19 17:30-05', s.precio, 'confirmada', 'en_linea'
FROM clientes.cliente c, barberos.barbero b, catalogo.servicio s
WHERE c.celular = '3115550958' AND b.nombres = 'Santiago' AND s.nombre = 'Corte de cabello';
DO $$ BEGIN RAISE NOTICE 'OK   una cita puede empezar justo cuando termina la anterior'; END $$;

-- Cancelar libera el horario.
UPDATE agenda.cita SET estado = 'cancelada', cancelada_en = now(), cancelada_por = 'cliente', motivo_cancelacion = 'solicitud_cliente', cancelacion_tardia = false
WHERE inicio = '2026-11-19 16:45-05';
INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen, creada_por)
SELECT c.id, b.id, s.id, '2026-11-19 16:45-05', '2026-11-19 17:15-05', s.precio, 'sin_confirmar', 'panel', u.id
FROM clientes.cliente c, barberos.barbero b, catalogo.servicio s, identidad.usuario u
WHERE c.celular = '3115550958' AND b.nombres = 'Santiago' AND s.nombre = 'Perfilado y diseño de barba' AND u.correo = 'laura@diamante.co';
DO $$ BEGIN RAISE NOTICE 'OK   cancelar una cita libera su horario'; END $$;

SELECT pg_temp.espera_error(
  $q$UPDATE agenda.cita SET estado = 'cancelada' WHERE inicio = '2026-11-19 15:30-05'$q$,
  '23514', 'una cita cancelada exige fecha, actor y marca de cancelación tardía');
SELECT pg_temp.espera_error(
  $q$INSERT INTO agenda.cita (cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen)
     SELECT c.id, b.id, s.id, '2026-11-20 10:00-05', '2026-11-20 10:45-05', 35000, 'sin_confirmar', 'panel'
     FROM clientes.cliente c, barberos.barbero b, catalogo.servicio s WHERE c.celular = '3105559917' AND b.nombres = 'Santiago' AND s.nombre = 'Corte de cabello'$q$,
  '23514', 'una cita creada en el panel registra quién la creó');

-- ---------- Ventas ----------
UPDATE ventas.contador_ticket SET ultimo = ultimo + 1 RETURNING ultimo \gset
BEGIN;
INSERT INTO ventas.venta (numero_ticket, registrada_en, registrada_por, barbero_id, cliente_id, cita_id, descuento_id, motivo_descuento_id, descuento_valor, propina, porcentaje_comision)
SELECT :ultimo, '2026-11-20 03:30+00', u.id, b.id, c.id, ci.id, 1, 1, 5500, 5000, b.porcentaje_comision
FROM identidad.usuario u, barberos.barbero b, clientes.cliente c, agenda.cita ci
WHERE u.correo = 'laura@diamante.co' AND b.nombres = 'Santiago' AND c.celular = '3105559917' AND ci.inicio = '2026-11-19 15:30-05'
RETURNING id AS venta_id \gset
INSERT INTO ventas.venta_linea (venta_id, posicion, servicio_id, precio) SELECT :venta_id, 1, id, precio FROM catalogo.servicio WHERE nombre = 'Combo cabello + barba';
INSERT INTO ventas.venta_pago (venta_id, metodo_pago_id, monto) SELECT :venta_id, id, 54500 FROM ventas.metodo_pago WHERE codigo = 'nequi';
COMMIT;
DO $$ BEGIN RAISE NOTICE 'OK   una venta cuadrada se confirma (55.000 − 5.500 + 5.000 = 54.500)'; END $$;

SELECT fecha_operacion = '2026-11-19' AS ok FROM ventas.venta WHERE numero_ticket = :ultimo \gset
\if :ok
  DO $$ BEGIN RAISE NOTICE 'OK   la fecha de operación usa la hora de Bogotá (03:30 UTC → 19 de noviembre)'; END $$;
\else
  DO $$ BEGIN RAISE EXCEPTION 'FALLA fecha de operación'; END $$;
\endif

SELECT subtotal = 55000 AND venta_neta = 49500 AND total_cobrado = 54500 AND comision = 24750 AS ok FROM ventas.v_venta WHERE numero_ticket = :ultimo \gset
\if :ok
  DO $$ BEGIN RAISE NOTICE 'OK   la vista v_venta calcula neta, total y comisión'; END $$;
\else
  DO $$ BEGIN RAISE EXCEPTION 'FALLA v_venta'; END $$;
\endif

CREATE OR REPLACE FUNCTION pg_temp.venta_descuadrada() RETURNS void LANGUAGE plpgsql AS $$
DECLARE v bigint;
BEGIN
  INSERT INTO ventas.venta (numero_ticket, registrada_por, barbero_id, porcentaje_comision, propina)
  SELECT 999, u.id, b.id, 50, 0 FROM identidad.usuario u, barberos.barbero b WHERE u.correo = 'laura@diamante.co' AND b.nombres = 'Santiago' RETURNING id INTO v;
  INSERT INTO ventas.venta_linea (venta_id, posicion, servicio_id, precio) SELECT v, 1, id, precio FROM catalogo.servicio WHERE nombre = 'Corte de cabello';
  INSERT INTO ventas.venta_pago (venta_id, metodo_pago_id, monto) SELECT v, id, 30000 FROM ventas.metodo_pago WHERE codigo = 'efectivo';
  SET CONSTRAINTS ALL IMMEDIATE;   -- fuerza la verificación diferida dentro de la prueba
END $$;
SELECT pg_temp.espera_error('SELECT pg_temp.venta_descuadrada()', '23514', 'una venta cuyos pagos no cuadran se rechaza');

CREATE OR REPLACE FUNCTION pg_temp.venta_sin_lineas() RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO ventas.venta (numero_ticket, registrada_por, barbero_id, porcentaje_comision)
  SELECT 998, u.id, b.id, 50 FROM identidad.usuario u, barberos.barbero b WHERE u.correo = 'laura@diamante.co' AND b.nombres = 'Santiago';
  SET CONSTRAINTS ALL IMMEDIATE;
END $$;
SELECT pg_temp.espera_error('SELECT pg_temp.venta_sin_lineas()', '23514', 'una venta sin servicios se rechaza');

SELECT pg_temp.espera_error(
  $q$UPDATE ventas.venta SET estado = 'anulada' WHERE numero_ticket = 1$q$,
  '23514', 'anular una venta exige fecha, responsable y motivo');

-- ---------- Funciones y vistas publicadas ----------
SELECT estado = 'vacaciones' AS ok FROM barberos.estados_en('2026-11-20') e JOIN barberos.barbero b ON b.id = e.barbero_id WHERE b.nombres = 'Mateo' \gset
\if :ok
  DO $$ BEGIN RAISE NOTICE 'OK   estados_en: Mateo está de vacaciones el 20 de noviembre'; END $$;
\else
  DO $$ BEGIN RAISE EXCEPTION 'FALLA estados_en'; END $$;
\endif
SELECT estado = 'activo' AS ok FROM barberos.estados_en('2026-12-01') e JOIN barberos.barbero b ON b.id = e.barbero_id WHERE b.nombres = 'Mateo' \gset
\if :ok
  DO $$ BEGIN RAISE NOTICE 'OK   estados_en: al terminar la ausencia vuelve solo a activo'; END $$;
\else
  DO $$ BEGIN RAISE EXCEPTION 'FALLA estados_en (regreso)'; END $$;
\endif
SELECT abre = '09:00' AND cierra = '20:00' AS ok FROM barberos.v_horario_atencion WHERE dia_semana = 2 \gset
\if :ok
  DO $$ BEGIN RAISE NOTICE 'OK   horario de atención del martes = 9:00 a 20:00 (mínimo y máximo de los barberos)'; END $$;
\else
  DO $$ BEGIN RAISE EXCEPTION 'FALLA v_horario_atencion'; END $$;
\endif

-- ---------- Auditoría y anonimización ----------
INSERT INTO auditoria.registro (actor, modulo, accion, entidad, entidad_id) VALUES ('sistema', 'pruebas', 'prueba', 'x', '1');
SELECT pg_temp.espera_error($q$UPDATE auditoria.registro SET accion = 'otra'$q$, 'P0001', 'la auditoría no se puede modificar');
SELECT pg_temp.espera_error($q$DELETE FROM auditoria.registro$q$, 'P0001', 'la auditoría no se puede borrar');
SELECT pg_temp.espera_error(
  $q$UPDATE clientes.cliente SET anonimizado_en = now() WHERE celular = '3115550958'$q$,
  '23514', 'anonimizar exige borrar todos los datos personales');
UPDATE clientes.cliente SET anonimizado_en = now(), celular = NULL, nombre = NULL, correo = NULL WHERE celular = '3115550958';
DO $$ BEGIN RAISE NOTICE 'OK   un cliente anonimizado conserva su historial sin datos personales'; END $$;
