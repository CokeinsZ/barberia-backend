-- Módulo `ventas` · creación del esquema
-- Dueño: backend/modulos/ventas. Solo este módulo escribe en el esquema `ventas`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- VENTAS (ventas, pagos, descuentos y cierres de caja)
-- =====================================================================
CREATE SCHEMA ventas;

CREATE TYPE ventas.tipo_descuento AS ENUM ('porcentaje', 'valor_fijo');
CREATE TYPE ventas.estado_venta   AS ENUM ('registrada', 'anulada');

CREATE TABLE ventas.metodo_pago (
  id      smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  codigo  text NOT NULL CHECK (codigo ~ '^[a-z_]{2,30}$'),
  nombre  text NOT NULL,
  activo  boolean NOT NULL DEFAULT true,
  orden   smallint NOT NULL DEFAULT 0,
  CONSTRAINT metodo_pago_codigo_uk UNIQUE (codigo)
);

CREATE TABLE ventas.descuento (
  id                    smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  nombre                text NOT NULL,
  tipo                  ventas.tipo_descuento NOT NULL,
  valor                 numeric(12,2) NOT NULL CHECK (valor > 0),
  permitido_a_barberos  boolean NOT NULL DEFAULT false,
  activo                boolean NOT NULL DEFAULT true,
  orden                 smallint NOT NULL DEFAULT 0,
  CONSTRAINT descuento_nombre_uk UNIQUE (nombre),
  CHECK (tipo <> 'porcentaje' OR valor <= 100)
);

CREATE TABLE ventas.motivo_descuento (
  id      smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  nombre  text NOT NULL,
  activo  boolean NOT NULL DEFAULT true,
  orden   smallint NOT NULL DEFAULT 0,
  CONSTRAINT motivo_descuento_nombre_uk UNIQUE (nombre)
);

-- Consecutivo sin huecos: se incrementa con bloqueo de fila dentro de la
-- transacción de la venta (una secuencia dejaría huecos al revertir).
CREATE TABLE ventas.contador_ticket (
  id      smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  ultimo  bigint NOT NULL DEFAULT 0 CHECK (ultimo >= 0)
);

CREATE TABLE ventas.venta (
  id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  numero_ticket        bigint NOT NULL,
  registrada_en        timestamptz NOT NULL DEFAULT now(),
  fecha_operacion      date GENERATED ALWAYS AS ((registrada_en AT TIME ZONE 'America/Bogota')::date) STORED,
  registrada_por       bigint NOT NULL REFERENCES identidad.usuario (id),
  barbero_id           bigint NOT NULL REFERENCES barberos.barbero (id),
  cliente_id           bigint REFERENCES clientes.cliente (id),   -- NULL = cliente ocasional
  cita_id              bigint REFERENCES agenda.cita (id),
  descuento_id         smallint REFERENCES ventas.descuento (id),
  motivo_descuento_id  smallint REFERENCES ventas.motivo_descuento (id),
  descuento_valor      nucleo.dinero NOT NULL DEFAULT 0,          -- valor aplicado (hecho histórico)
  propina              nucleo.dinero NOT NULL DEFAULT 0,
  porcentaje_comision  nucleo.porcentaje NOT NULL,                -- copia del % vigente del barbero
  estado               ventas.estado_venta NOT NULL DEFAULT 'registrada',
  anulada_en           timestamptz,
  anulada_por          bigint REFERENCES identidad.usuario (id),
  motivo_anulacion     text,
  CONSTRAINT venta_ticket_uk UNIQUE (numero_ticket),
  CONSTRAINT venta_cita_uk   UNIQUE (cita_id),
  CONSTRAINT venta_descuento_ck CHECK (
    (descuento_id IS NULL) = (motivo_descuento_id IS NULL)
    AND (descuento_id IS NULL) = (descuento_valor = 0)
  ),
  CONSTRAINT venta_anulacion_ck CHECK (
    (estado = 'anulada') = (anulada_en IS NOT NULL)
    AND (anulada_en IS NULL) = (anulada_por IS NULL)
    AND (anulada_en IS NULL) = (motivo_anulacion IS NULL)
  )
);
CREATE INDEX venta_fecha_idx         ON ventas.venta (fecha_operacion) WHERE estado = 'registrada';
CREATE INDEX venta_barbero_fecha_idx ON ventas.venta (barbero_id, fecha_operacion) WHERE estado = 'registrada';
CREATE INDEX venta_cliente_fecha_idx ON ventas.venta (cliente_id, fecha_operacion DESC) WHERE cliente_id IS NOT NULL;

CREATE TABLE ventas.venta_linea (
  venta_id     bigint NOT NULL REFERENCES ventas.venta (id) ON DELETE CASCADE,
  posicion     smallint NOT NULL CHECK (posicion BETWEEN 1 AND 20),
  servicio_id  bigint NOT NULL REFERENCES catalogo.servicio (id),
  precio       nucleo.dinero NOT NULL,                     -- precio vigente al vender
  PRIMARY KEY (venta_id, posicion)
);
CREATE INDEX venta_linea_servicio_idx ON ventas.venta_linea (servicio_id);

-- Una venta se puede pagar con uno o más métodos (el MVP usa uno).
CREATE TABLE ventas.venta_pago (
  venta_id        bigint NOT NULL REFERENCES ventas.venta (id) ON DELETE CASCADE,
  metodo_pago_id  smallint NOT NULL REFERENCES ventas.metodo_pago (id),
  monto           nucleo.dinero NOT NULL CHECK (monto > 0),
  PRIMARY KEY (venta_id, metodo_pago_id)
);
CREATE INDEX venta_pago_metodo_idx ON ventas.venta_pago (metodo_pago_id);

-- Cuadre de la venta, verificado al confirmar la transacción:
-- al menos una línea; descuento ≤ subtotal; pagos = subtotal − descuento + propina.
CREATE FUNCTION ventas.verificar_cuadre() RETURNS trigger
  LANGUAGE plpgsql AS $$
DECLARE
  v_id        bigint;
  v_desc      bigint;
  v_propina   bigint;
  v_lineas    integer;
  v_subtotal  bigint;
  v_pagos     bigint;
BEGIN
  -- Cada rama se resuelve por separado: NEW.venta_id no existe en la tabla venta.
  IF TG_TABLE_NAME = 'venta' THEN
    v_id := NEW.id;
  ELSIF TG_OP = 'DELETE' THEN
    v_id := OLD.venta_id;
  ELSE
    v_id := NEW.venta_id;
  END IF;

  SELECT descuento_valor, propina INTO v_desc, v_propina FROM ventas.venta WHERE id = v_id;
  IF NOT FOUND THEN
    RETURN NULL;   -- la venta ya no existe (borrado en cascada)
  END IF;
  SELECT count(*), coalesce(sum(precio), 0) INTO v_lineas, v_subtotal FROM ventas.venta_linea WHERE venta_id = v_id;
  SELECT coalesce(sum(monto), 0) INTO v_pagos FROM ventas.venta_pago WHERE venta_id = v_id;
  IF v_lineas = 0 THEN
    RAISE EXCEPTION 'La venta % no tiene servicios', v_id USING ERRCODE = 'check_violation';
  END IF;
  IF v_desc > v_subtotal THEN
    RAISE EXCEPTION 'El descuento de la venta % supera el subtotal', v_id USING ERRCODE = 'check_violation';
  END IF;
  IF v_pagos <> v_subtotal - v_desc + v_propina THEN
    RAISE EXCEPTION 'Los pagos de la venta % (%) no cuadran con el total (%)', v_id, v_pagos, v_subtotal - v_desc + v_propina
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER venta_cuadre_trg AFTER INSERT OR UPDATE ON ventas.venta
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();
CREATE CONSTRAINT TRIGGER venta_linea_cuadre_trg AFTER INSERT OR UPDATE OR DELETE ON ventas.venta_linea
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();
CREATE CONSTRAINT TRIGGER venta_pago_cuadre_trg AFTER INSERT OR UPDATE OR DELETE ON ventas.venta_pago
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();

CREATE TABLE ventas.cierre_caja (
  id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  fecha_operacion  date NOT NULL,
  cerrado_por      bigint NOT NULL REFERENCES identidad.usuario (id),
  cerrado_en       timestamptz NOT NULL DEFAULT now(),
  observacion      text,
  CONSTRAINT cierre_caja_fecha_uk UNIQUE (fecha_operacion)
);

-- Lo esperado se guarda como foto del momento del cierre; la diferencia
-- (contado − esperado) se calcula al consultar.
CREATE TABLE ventas.cierre_caja_detalle (
  cierre_id       bigint NOT NULL REFERENCES ventas.cierre_caja (id) ON DELETE CASCADE,
  metodo_pago_id  smallint NOT NULL REFERENCES ventas.metodo_pago (id),
  esperado        nucleo.dinero NOT NULL,
  contado         nucleo.dinero,                         -- NULL = no verificado
  PRIMARY KEY (cierre_id, metodo_pago_id)
);

-- Vista publicada: totales de cada venta vigente (contrato de lectura de
-- reportes y del dashboard).
CREATE VIEW ventas.v_venta AS
SELECT v.id,
       v.numero_ticket,
       v.fecha_operacion,
       v.registrada_en,
       v.barbero_id,
       v.cliente_id,
       v.cita_id,
       l.subtotal,
       v.descuento_valor,
       l.subtotal - v.descuento_valor                                          AS venta_neta,
       v.propina,
       l.subtotal - v.descuento_valor + v.propina                              AS total_cobrado,
       v.porcentaje_comision,
       round((l.subtotal - v.descuento_valor) * v.porcentaje_comision / 100)::bigint AS comision
FROM ventas.venta v
CROSS JOIN LATERAL (
  SELECT sum(vl.precio)::bigint AS subtotal FROM ventas.venta_linea vl WHERE vl.venta_id = v.id
) l
WHERE v.estado = 'registrada';


-- Datos iniciales.
INSERT INTO ventas.contador_ticket DEFAULT VALUES;
INSERT INTO ventas.metodo_pago (codigo, nombre, activo, orden) VALUES
  ('efectivo', 'Efectivo', true, 1),
  ('nequi', 'Nequi', true, 2),
  ('daviplata', 'Daviplata', true, 3),
  ('tarjeta', 'Tarjeta', true, 4),
  ('transferencia', 'Transferencia', true, 5),
  ('link_pago', 'Link de pago', false, 6);
INSERT INTO ventas.descuento (nombre, tipo, valor, permitido_a_barberos, orden) VALUES
  ('10 %', 'porcentaje', 10, true, 1),
  ('15 %', 'porcentaje', 15, true, 2),
  ('$5.000', 'valor_fijo', 5000, true, 3);
INSERT INTO ventas.motivo_descuento (nombre, orden) VALUES
  ('Cliente frecuente', 1), ('Promoción vigente', 2), ('Cortesía de la casa', 3), ('Otro motivo', 4);
