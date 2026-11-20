-- Módulo `trabajos` · creación del esquema
-- Dueño: backend/modulos/trabajos. Solo este módulo escribe en el esquema `trabajos`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- TRABAJOS (cola de tareas en segundo plano)
-- =====================================================================
CREATE SCHEMA trabajos;

CREATE TYPE trabajos.estado AS ENUM ('pendiente', 'en_proceso', 'completado', 'fallido');

CREATE TABLE trabajos.trabajo (
  id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  tipo          text NOT NULL,
  carga         jsonb NOT NULL DEFAULT '{}',
  estado        trabajos.estado NOT NULL DEFAULT 'pendiente',
  ejecutar_en   timestamptz NOT NULL DEFAULT now(),
  intentos      smallint NOT NULL DEFAULT 0 CHECK (intentos >= 0),
  max_intentos  smallint NOT NULL DEFAULT 5 CHECK (max_intentos >= 1),
  tomado_en     timestamptz,
  tomado_por    text,
  ultimo_error  text,
  clave_unica   text,                         -- idempotencia de tareas programadas
  creado_en     timestamptz NOT NULL DEFAULT now(),
  terminado_en  timestamptz,
  CONSTRAINT trabajo_clave_uk UNIQUE (clave_unica)
);
CREATE INDEX trabajo_listo_idx      ON trabajos.trabajo (ejecutar_en) WHERE estado = 'pendiente';
CREATE INDEX trabajo_en_proceso_idx ON trabajos.trabajo (tomado_en)   WHERE estado = 'en_proceso';
