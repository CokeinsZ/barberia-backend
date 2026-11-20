-- Módulo `auditoria` · creación del esquema
-- Dueño: backend/modulos/auditoria. Solo este módulo escribe en el esquema `auditoria`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- AUDITORÍA (solo inserción)
-- =====================================================================
CREATE SCHEMA auditoria;

CREATE TABLE auditoria.registro (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  ocurrido_en  timestamptz NOT NULL DEFAULT now(),
  usuario_id   bigint REFERENCES identidad.usuario (id),
  actor        nucleo.actor NOT NULL,
  modulo       text NOT NULL,
  accion       text NOT NULL,                 -- p. ej., barbero.estado_cambiado
  entidad      text NOT NULL,
  entidad_id   text NOT NULL,
  antes        jsonb,
  despues      jsonb,
  ip           inet
);
CREATE INDEX registro_entidad_idx ON auditoria.registro (entidad, entidad_id, ocurrido_en DESC);
CREATE INDEX registro_tiempo_brin ON auditoria.registro USING brin (ocurrido_en);   -- tabla de solo inserción, ordenada en el tiempo

CREATE FUNCTION auditoria.impedir_cambios() RETURNS trigger
  LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'auditoria.registro es de solo inserción';
END $$;

CREATE TRIGGER registro_solo_insercion BEFORE UPDATE OR DELETE OR TRUNCATE ON auditoria.registro
  FOR EACH STATEMENT EXECUTE FUNCTION auditoria.impedir_cambios();

-- El rol de la aplicación nunca modifica la auditoría (si el rol existe).
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'diamante_app') THEN
    REVOKE UPDATE, DELETE, TRUNCATE ON auditoria.registro FROM diamante_app;
  END IF;
END $$;
