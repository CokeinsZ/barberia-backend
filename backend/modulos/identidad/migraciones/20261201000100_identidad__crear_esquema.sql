-- Módulo `identidad` · creación del esquema
-- Dueño: backend/modulos/identidad. Solo este módulo escribe en el esquema `identidad`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- IDENTIDAD (cuentas, sesiones y tokens del personal)
-- =====================================================================
CREATE SCHEMA identidad;

CREATE TYPE identidad.rol             AS ENUM ('administrador', 'barbero');
CREATE TYPE identidad.proposito_token AS ENUM ('activacion', 'recuperacion');

CREATE TABLE identidad.usuario (
  id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  correo             nucleo.correo NOT NULL,
  hash_contrasena    text,                         -- Argon2id; NULL hasta activar la cuenta
  rol                identidad.rol NOT NULL,
  activo             boolean NOT NULL DEFAULT true,
  intentos_fallidos  smallint NOT NULL DEFAULT 0 CHECK (intentos_fallidos >= 0),
  bloqueado_hasta    timestamptz,
  ultimo_acceso_en   timestamptz,
  creado_en          timestamptz NOT NULL DEFAULT now(),
  actualizado_en     timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT usuario_correo_uk UNIQUE (correo),
  CONSTRAINT usuario_id_rol_uk UNIQUE (id, rol)    -- permite que los subtipos validen el rol
);

-- Subtipo: administrador (los barberos se modelan en barberos.barbero).
CREATE TABLE identidad.administrador (
  usuario_id  bigint PRIMARY KEY,
  rol         identidad.rol NOT NULL DEFAULT 'administrador' CHECK (rol = 'administrador'),
  nombre      text NOT NULL CHECK (length(nombre) BETWEEN 2 AND 120),
  CONSTRAINT administrador_usuario_fk FOREIGN KEY (usuario_id, rol)
    REFERENCES identidad.usuario (id, rol)
);

CREATE TABLE identidad.sesion (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  usuario_id      bigint NOT NULL REFERENCES identidad.usuario (id),
  token_hash      bytea NOT NULL CHECK (octet_length(token_hash) = 32),   -- SHA-256 del token de la cookie
  recordarme      boolean NOT NULL DEFAULT false,
  creada_en       timestamptz NOT NULL DEFAULT now(),
  ultimo_uso_en   timestamptz NOT NULL DEFAULT now(),
  expira_en       timestamptz NOT NULL,
  revocada_en     timestamptz,
  ip              inet,
  agente_usuario  text,
  CONSTRAINT sesion_token_uk UNIQUE (token_hash),
  CHECK (expira_en > creada_en)
);
CREATE INDEX sesion_usuario_vigente_idx ON identidad.sesion (usuario_id) WHERE revocada_en IS NULL;
CREATE INDEX sesion_expira_idx          ON identidad.sesion (expira_en);

CREATE TABLE identidad.token_usuario (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  usuario_id  bigint NOT NULL REFERENCES identidad.usuario (id),
  proposito   identidad.proposito_token NOT NULL,
  token_hash  bytea NOT NULL CHECK (octet_length(token_hash) = 32),
  creado_en   timestamptz NOT NULL DEFAULT now(),
  expira_en   timestamptz NOT NULL,
  usado_en    timestamptz,
  CONSTRAINT token_usuario_token_uk UNIQUE (token_hash),
  CHECK (expira_en > creado_en)
);
CREATE INDEX token_usuario_vigente_idx ON identidad.token_usuario (usuario_id, proposito) WHERE usado_en IS NULL;

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON identidad.usuario
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
