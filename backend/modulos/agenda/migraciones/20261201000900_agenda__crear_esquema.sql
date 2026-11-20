-- Módulo `agenda` · creación del esquema
-- Dueño: backend/modulos/agenda. Solo este módulo escribe en el esquema `agenda`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- AGENDA (citas, configuración de agendamiento y recordatorios)
-- =====================================================================
CREATE SCHEMA agenda;

CREATE TYPE agenda.estado_cita            AS ENUM ('sin_confirmar', 'confirmada', 'en_curso', 'atendida', 'no_asistio', 'cancelada');
CREATE TYPE agenda.origen_cita            AS ENUM ('en_linea', 'panel');
CREATE TYPE agenda.motivo_cancelacion     AS ENUM ('solicitud_cliente', 'barbero_no_disponible', 'otro');
CREATE TYPE agenda.evento_cita            AS ENUM ('creada', 'confirmada', 'iniciada', 'atendida', 'no_asistio',
                                                   'cancelada', 'reprogramada', 'reasignada');
CREATE TYPE agenda.proposito_mensaje      AS ENUM ('confirmacion', 'recordatorio_24h', 'recordatorio_2h', 'cancelacion', 'reprogramacion');
CREATE TYPE agenda.politica_inasistencias AS ENUM ('confirmar_por_llamada', 'bloquear_reserva_en_linea', 'sin_restriccion');

-- Una sola fila.
CREATE TABLE agenda.configuracion (
  id                       smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  anticipacion_minima_min  smallint NOT NULL DEFAULT 120  CHECK (anticipacion_minima_min BETWEEN 0 AND 1440),
  ventana_reserva_dias     smallint NOT NULL DEFAULT 30   CHECK (ventana_reserva_dias BETWEEN 1 AND 180),
  intervalo_min            smallint NOT NULL DEFAULT 30   CHECK (intervalo_min IN (15, 30)),
  aviso_cancelacion_min    smallint NOT NULL DEFAULT 180  CHECK (aviso_cancelacion_min BETWEEN 0 AND 2880),
  recordatorio_24h         boolean NOT NULL DEFAULT true,
  recordatorio_2h          boolean NOT NULL DEFAULT true,
  politica_inasistencias   agenda.politica_inasistencias NOT NULL DEFAULT 'confirmar_por_llamada',
  umbral_inasistencias     smallint NOT NULL DEFAULT 2    CHECK (umbral_inasistencias BETWEEN 1 AND 10),
  actualizado_en           timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE agenda.cita (
  id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  cliente_id             bigint NOT NULL REFERENCES clientes.cliente (id),
  barbero_id             bigint NOT NULL REFERENCES barberos.barbero (id),
  servicio_id            bigint NOT NULL REFERENCES catalogo.servicio (id),
  inicio                 timestamptz NOT NULL,
  fin                    timestamptz NOT NULL,
  precio                 nucleo.dinero NOT NULL,          -- precio vigente al agendar
  estado                 agenda.estado_cita NOT NULL,
  origen                 agenda.origen_cita NOT NULL,
  asignacion_automatica  boolean NOT NULL DEFAULT false,  -- «Cualquier barbero»
  nota                   text CHECK (length(nota) <= 500),
  creada_por             bigint REFERENCES identidad.usuario (id),   -- NULL si la creó el cliente
  creada_en              timestamptz NOT NULL DEFAULT now(),
  actualizado_en         timestamptz NOT NULL DEFAULT now(),
  cancelada_en           timestamptz,
  cancelada_por          nucleo.actor,
  motivo_cancelacion     agenda.motivo_cancelacion,
  cancelacion_tardia     boolean,
  CHECK (fin > inicio),
  CHECK (fin - inicio <= interval '8 hours'),
  CONSTRAINT cita_origen_ck CHECK ((origen = 'panel') = (creada_por IS NOT NULL)),
  CONSTRAINT cita_cancelacion_ck CHECK (
    (estado = 'cancelada') = (cancelada_en IS NOT NULL)
    AND (cancelada_en IS NULL) = (cancelada_por IS NULL)
    AND (cancelada_en IS NULL) = (cancelacion_tardia IS NULL)
  ),
  -- Garantía central del sistema: dos citas vigentes del mismo barbero nunca
  -- se cruzan, aunque lleguen solicitudes simultáneas (RNF-INT-01).
  CONSTRAINT cita_sin_solape EXCLUDE USING gist (
    barbero_id WITH =,
    tstzrange(inicio, fin) WITH &&
  ) WHERE (estado NOT IN ('cancelada', 'no_asistio'))
);
-- (barbero_id, inicio) también resuelve la agenda del día de todos los
-- barberos gracias al skip scan de PostgreSQL 18; no hace falta un índice solo por inicio.
CREATE INDEX cita_barbero_inicio_idx     ON agenda.cita (barbero_id, inicio);
CREATE INDEX cita_cliente_inicio_idx     ON agenda.cita (cliente_id, inicio DESC);
CREATE INDEX cita_por_recordar_idx       ON agenda.cita (inicio) WHERE estado IN ('sin_confirmar', 'confirmada');

-- Vista publicada: citas sin datos de contacto (contrato de lectura).
CREATE VIEW agenda.v_cita AS
SELECT id, cliente_id, barbero_id, servicio_id, inicio, fin, precio, estado, origen,
       asignacion_automatica, cancelacion_tardia
FROM agenda.cita;

CREATE TABLE agenda.enlace_gestion (
  cita_id     bigint PRIMARY KEY REFERENCES agenda.cita (id) ON DELETE CASCADE,
  token_hash  bytea NOT NULL CHECK (octet_length(token_hash) = 32),
  creado_en   timestamptz NOT NULL DEFAULT now(),
  expira_en   timestamptz NOT NULL,
  CONSTRAINT enlace_gestion_token_uk UNIQUE (token_hash)
);

CREATE TABLE agenda.historial_cita (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  cita_id         bigint NOT NULL REFERENCES agenda.cita (id) ON DELETE CASCADE,
  evento          agenda.evento_cita NOT NULL,
  actor           nucleo.actor NOT NULL,
  usuario_id      bigint REFERENCES identidad.usuario (id),
  inicio_previo   timestamptz,
  barbero_previo  bigint REFERENCES barberos.barbero (id),
  tardio          boolean NOT NULL DEFAULT false,
  detalle         text,
  ocurrido_en     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX historial_cita_idx ON agenda.historial_cita (cita_id, ocurrido_en);

-- Relación cita ↔ mensaje. El índice único hace idempotentes los
-- recordatorios: uno por cita, tipo y hora de inicio (si se reprograma,
-- la nueva hora genera recordatorios nuevos).
CREATE TABLE agenda.mensaje_cita (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  cita_id      bigint NOT NULL REFERENCES agenda.cita (id) ON DELETE CASCADE,
  proposito    agenda.proposito_mensaje NOT NULL,
  inicio_cita  timestamptz NOT NULL,
  mensaje_id   bigint NOT NULL REFERENCES notificaciones.mensaje (id),
  CONSTRAINT mensaje_cita_mensaje_uk UNIQUE (mensaje_id)
);
CREATE UNIQUE INDEX mensaje_cita_recordatorio_uk ON agenda.mensaje_cita (cita_id, proposito, inicio_cita)
  WHERE proposito IN ('recordatorio_24h', 'recordatorio_2h');

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON agenda.configuracion
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON agenda.cita
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();

-- Datos iniciales.
INSERT INTO agenda.configuracion DEFAULT VALUES;
