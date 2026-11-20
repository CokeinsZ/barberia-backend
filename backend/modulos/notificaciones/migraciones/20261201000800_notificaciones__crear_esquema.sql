-- Módulo `notificaciones` · creación del esquema
-- Dueño: backend/modulos/notificaciones. Solo este módulo escribe en el esquema `notificaciones`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- NOTIFICACIONES (mensajes salientes, webhooks y avisos internos)
-- =====================================================================
CREATE SCHEMA notificaciones;

CREATE TYPE notificaciones.canal          AS ENUM ('whatsapp', 'correo');
CREATE TYPE notificaciones.estado_mensaje AS ENUM ('pendiente', 'enviado', 'entregado', 'leido', 'fallido', 'cancelado');
CREATE TYPE notificaciones.tipo_aviso     AS ENUM ('cambio_horario_con_citas', 'bloqueo_con_citas', 'ausencia_con_citas',
                                                   'solicitud_correccion', 'mensaje_fallido');

-- Cola de salida (patrón outbox): se inserta en la misma transacción que el
-- hecho que la origina y la procesa el worker.
CREATE TABLE notificaciones.mensaje (
  id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  canal             notificaciones.canal NOT NULL,
  plantilla         text NOT NULL,                         -- p. ej., cita_confirmada_v1
  destinatario      text NOT NULL,                         -- celular o correo
  variables         jsonb NOT NULL DEFAULT '{}',
  contiene_secreto  boolean NOT NULL DEFAULT false,        -- si es true, las variables se borran al enviar
  estado            notificaciones.estado_mensaje NOT NULL DEFAULT 'pendiente',
  programado_para   timestamptz NOT NULL DEFAULT now(),
  intentos          smallint NOT NULL DEFAULT 0 CHECK (intentos >= 0),
  id_proveedor      text,                                  -- wamid de Meta o Message-ID del correo
  ultimo_error      text,
  creado_en         timestamptz NOT NULL DEFAULT now(),
  enviado_en        timestamptz,
  actualizado_en    timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT mensaje_proveedor_uk UNIQUE (id_proveedor)
);
CREATE INDEX mensaje_pendiente_idx    ON notificaciones.mensaje (programado_para) WHERE estado = 'pendiente';
CREATE INDEX mensaje_destinatario_idx ON notificaciones.mensaje (destinatario, creado_en DESC);

CREATE TABLE notificaciones.evento_webhook (
  id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  recibido_en   timestamptz NOT NULL DEFAULT now(),
  carga         jsonb NOT NULL,
  procesado_en  timestamptz,
  error         text
);
CREATE INDEX evento_webhook_pendiente_idx ON notificaciones.evento_webhook (recibido_en) WHERE procesado_en IS NULL;

CREATE TABLE notificaciones.aviso (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  tipo       notificaciones.tipo_aviso NOT NULL,
  titulo     text NOT NULL,
  detalle    text,
  datos      jsonb NOT NULL DEFAULT '{}',
  creado_en  timestamptz NOT NULL DEFAULT now(),
  leido_en   timestamptz,
  leido_por  bigint REFERENCES identidad.usuario (id),
  CHECK ((leido_en IS NULL) = (leido_por IS NULL))
);
CREATE INDEX aviso_no_leido_idx ON notificaciones.aviso (creado_en DESC) WHERE leido_en IS NULL;

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON notificaciones.mensaje
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
