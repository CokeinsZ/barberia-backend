-- Módulo `clientes` · creación del esquema
-- Dueño: backend/modulos/clientes. Solo este módulo escribe en el esquema `clientes`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- CLIENTES (clientes, autorizaciones y proyección de métricas)
-- =====================================================================
CREATE SCHEMA clientes;

CREATE TYPE clientes.canal_autorizacion     AS ENUM ('en_linea', 'panel');
CREATE TYPE clientes.segmento               AS ENUM ('nuevo', 'recurrente', 'frecuente', 'inactivo');
CREATE TYPE clientes.tipo_solicitud_titular AS ENUM ('consulta', 'actualizacion', 'supresion');

CREATE TABLE clientes.cliente (
  id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  celular                nucleo.celular_co,                -- único; NULL solo si se anonimizó
  nombre                 text CHECK (length(nombre) BETWEEN 2 AND 120),
  correo                 nucleo.correo,
  fecha_nacimiento       date,
  preferencias           text CHECK (length(preferencias) <= 500),
  recordatorios_activos  boolean NOT NULL DEFAULT true,
  creado_en              timestamptz NOT NULL DEFAULT now(),
  actualizado_en         timestamptz NOT NULL DEFAULT now(),
  anonimizado_en         timestamptz,
  CONSTRAINT cliente_celular_uk UNIQUE (celular),
  CONSTRAINT cliente_identificado_ck CHECK (anonimizado_en IS NOT NULL OR (celular IS NOT NULL AND nombre IS NOT NULL)),
  CONSTRAINT cliente_anonimizado_ck  CHECK (anonimizado_en IS NULL OR num_nonnulls(celular, nombre, correo, fecha_nacimiento, preferencias) = 0)
);
CREATE INDEX cliente_nombre_trgm_idx  ON clientes.cliente USING gin (nucleo.normalizar(nombre) gin_trgm_ops);
CREATE INDEX cliente_celular_trgm_idx ON clientes.cliente USING gin (celular gin_trgm_ops);
CREATE INDEX cliente_correo_idx       ON clientes.cliente (correo) WHERE correo IS NOT NULL;
CREATE INDEX cliente_cumpleanos_idx   ON clientes.cliente ((extract(month FROM fecha_nacimiento)), (extract(day FROM fecha_nacimiento)))
  WHERE fecha_nacimiento IS NOT NULL;

CREATE TABLE clientes.autorizacion (
  id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  cliente_id           bigint NOT NULL REFERENCES clientes.cliente (id),
  politica_version_id  smallint NOT NULL REFERENCES negocio.politica_version (id),
  canal                clientes.canal_autorizacion NOT NULL,
  otorgada_en          timestamptz NOT NULL DEFAULT now(),
  ip                   inet,
  agente_usuario       text,
  revocada_en          timestamptz
);
CREATE INDEX autorizacion_cliente_idx ON clientes.autorizacion (cliente_id, otorgada_en DESC);

CREATE TABLE clientes.solicitud_titular (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  cliente_id      bigint REFERENCES clientes.cliente (id),
  tipo            clientes.tipo_solicitud_titular NOT NULL,
  descripcion     text NOT NULL,
  recibida_en     timestamptz NOT NULL DEFAULT now(),
  respondida_en   timestamptz,
  respondida_por  bigint REFERENCES identidad.usuario (id),
  respuesta       text,
  CHECK ((respondida_en IS NULL) = (respuesta IS NULL))
);
CREATE INDEX solicitud_titular_abierta_idx ON clientes.solicitud_titular (recibida_en) WHERE respondida_en IS NULL;

-- Una sola fila (DA-07).
CREATE TABLE clientes.configuracion_segmentos (
  id                      smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  max_visitas_nuevo       smallint NOT NULL DEFAULT 2,
  max_visitas_recurrente  smallint NOT NULL DEFAULT 5,
  dias_inactividad        smallint NOT NULL DEFAULT 60,
  actualizado_en          timestamptz NOT NULL DEFAULT now(),
  CHECK (max_visitas_nuevo >= 1 AND max_visitas_recurrente > max_visitas_nuevo),
  CHECK (dias_inactividad BETWEEN 15 AND 365)
);

-- Proyección de lectura mantenida por eventos de ventas y agenda
-- (VentaRegistrada, VentaAnulada, InasistenciaMarcada) en la misma
-- transacción. Es la única desnormalización del modelo y es deliberada:
-- hace que el listado y los segmentos se consulten sin agregaciones.
CREATE TABLE clientes.metricas (
  cliente_id     bigint PRIMARY KEY REFERENCES clientes.cliente (id) ON DELETE CASCADE,
  visitas        integer NOT NULL DEFAULT 0 CHECK (visitas >= 0),
  total_gastado  nucleo.dinero NOT NULL DEFAULT 0,
  ultima_visita  date,
  inasistencias  integer NOT NULL DEFAULT 0 CHECK (inasistencias >= 0)
);
CREATE INDEX metricas_ultima_visita_idx ON clientes.metricas (ultima_visita);
CREATE INDEX metricas_visitas_idx       ON clientes.metricas (visitas);

CREATE TABLE clientes.visitas_por_barbero (
  cliente_id  bigint NOT NULL REFERENCES clientes.cliente (id) ON DELETE CASCADE,
  barbero_id  bigint NOT NULL REFERENCES barberos.barbero (id),
  visitas     integer NOT NULL CHECK (visitas >= 0),
  PRIMARY KEY (cliente_id, barbero_id)
);

-- Vista publicada: cliente con métricas y barbero habitual. El segmento se
-- calcula en la aplicación con la fecha de su reloj y la configuración.
CREATE VIEW clientes.v_cliente_resumen AS
SELECT c.id,
       c.celular,
       c.nombre,
       c.correo,
       c.fecha_nacimiento,
       coalesce(m.visitas, 0)       AS visitas,
       coalesce(m.total_gastado, 0) AS total_gastado,
       m.ultima_visita,
       coalesce(m.inasistencias, 0) AS inasistencias,
       h.barbero_id                 AS barbero_habitual_id
FROM clientes.cliente c
LEFT JOIN clientes.metricas m ON m.cliente_id = c.id
LEFT JOIN LATERAL (
  SELECT vb.barbero_id
  FROM clientes.visitas_por_barbero vb
  WHERE vb.cliente_id = c.id
  ORDER BY vb.visitas DESC, vb.barbero_id
  LIMIT 1
) h ON true
WHERE c.anonimizado_en IS NULL;

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON clientes.cliente
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON clientes.configuracion_segmentos
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();

-- Datos iniciales.
INSERT INTO clientes.configuracion_segmentos DEFAULT VALUES;
