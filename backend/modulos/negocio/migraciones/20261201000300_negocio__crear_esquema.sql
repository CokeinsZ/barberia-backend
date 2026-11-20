-- Módulo `negocio` · creación del esquema
-- Dueño: backend/modulos/negocio. Solo este módulo escribe en el esquema `negocio`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- NEGOCIO (datos de la barbería, días especiales y políticas legales)
-- =====================================================================
CREATE SCHEMA negocio;

CREATE TYPE negocio.tipo_dia_especial   AS ENUM ('festivo', 'cerrado', 'cierre_temprano');
CREATE TYPE negocio.origen_dia_especial AS ENUM ('automatico', 'manual');
CREATE TYPE negocio.tipo_politica       AS ENUM ('tratamiento_datos', 'cancelacion', 'terminos');

-- Una sola fila.
CREATE TABLE negocio.negocio (
  id                           smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  nombre_comercial             text NOT NULL,
  nit                          text,
  direccion                    text,
  zona_referencia              text,
  whatsapp                     nucleo.celular_co,
  instagram                    text CHECK (instagram ~ '^[A-Za-z0-9._]{1,30}$'),
  porcentaje_comision_defecto  nucleo.porcentaje NOT NULL DEFAULT 50,
  actualizado_en               timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE negocio.dia_especial (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  fecha        date NOT NULL,
  nombre       text NOT NULL CHECK (length(nombre) BETWEEN 2 AND 80),
  tipo         negocio.tipo_dia_especial NOT NULL,
  hora_cierre  time,
  origen       negocio.origen_dia_especial NOT NULL DEFAULT 'manual',
  creado_en    timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT dia_especial_fecha_uk UNIQUE (fecha),
  CONSTRAINT dia_especial_hora_cierre_ck CHECK ((tipo = 'cierre_temprano') = (hora_cierre IS NOT NULL))
);

CREATE TABLE negocio.politica_version (
  id             smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  tipo           negocio.tipo_politica NOT NULL,
  version        text NOT NULL,
  vigente_desde  timestamptz NOT NULL,
  url            text NOT NULL,
  CONSTRAINT politica_version_uk UNIQUE (tipo, version)
);
CREATE INDEX politica_vigente_idx ON negocio.politica_version (tipo, vigente_desde DESC);

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON negocio.negocio
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();

-- Datos iniciales.
INSERT INTO negocio.negocio (nombre_comercial) VALUES ('Diamante Barbería');
