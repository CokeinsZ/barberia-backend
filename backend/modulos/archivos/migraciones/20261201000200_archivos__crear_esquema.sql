-- Módulo `archivos` · creación del esquema
-- Dueño: backend/modulos/archivos. Solo este módulo escribe en el esquema `archivos`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- ARCHIVOS (metadatos; el contenido vive en el volumen de archivos)
-- =====================================================================
CREATE SCHEMA archivos;

CREATE TYPE archivos.visibilidad AS ENUM ('publico', 'privado');

CREATE TABLE archivos.archivo (
  id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  clave            text NOT NULL,                  -- ruta relativa en el almacenamiento
  nombre_original  text NOT NULL,
  tipo_mime        text NOT NULL CHECK (tipo_mime IN ('image/jpeg', 'image/png', 'image/webp', 'application/pdf')),
  tamano_bytes     integer NOT NULL CHECK (tamano_bytes BETWEEN 1 AND 10485760),   -- máximo 10 MB
  sha256           bytea NOT NULL CHECK (octet_length(sha256) = 32),
  visibilidad      archivos.visibilidad NOT NULL,
  subido_por       bigint REFERENCES identidad.usuario (id),
  subido_en        timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT archivo_clave_uk UNIQUE (clave)
);
