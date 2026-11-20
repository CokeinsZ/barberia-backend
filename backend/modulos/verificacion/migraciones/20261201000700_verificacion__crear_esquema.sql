-- Módulo `verificacion` · creación del esquema
-- Dueño: backend/modulos/verificacion. Solo este módulo escribe en el esquema `verificacion`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- VERIFICACIÓN (códigos por WhatsApp)
-- =====================================================================
CREATE SCHEMA verificacion;

CREATE TABLE verificacion.codigo (
  id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  celular                nucleo.celular_co NOT NULL,
  codigo_hash            bytea NOT NULL CHECK (octet_length(codigo_hash) = 32),   -- HMAC-SHA256 con clave del servidor
  intentos               smallint NOT NULL DEFAULT 0,
  max_intentos           smallint NOT NULL DEFAULT 5,
  creado_en              timestamptz NOT NULL DEFAULT now(),
  expira_en              timestamptz NOT NULL,
  verificado_en          timestamptz,
  comprobante_hash       bytea CHECK (octet_length(comprobante_hash) = 32),       -- token que autoriza crear la cita
  comprobante_expira_en  timestamptz,
  comprobante_usado_en   timestamptz,
  ip                     inet,
  CONSTRAINT codigo_comprobante_uk UNIQUE (comprobante_hash),
  CHECK (expira_en > creado_en),
  CHECK (intentos BETWEEN 0 AND max_intentos),
  CHECK ((verificado_en IS NULL) = (comprobante_hash IS NULL)),
  CHECK ((comprobante_hash IS NULL) = (comprobante_expira_en IS NULL))
);
CREATE INDEX codigo_celular_idx ON verificacion.codigo (celular, creado_en DESC);
CREATE INDEX codigo_ip_idx      ON verificacion.codigo (ip, creado_en DESC) WHERE ip IS NOT NULL;
