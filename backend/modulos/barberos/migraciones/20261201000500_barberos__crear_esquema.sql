-- Módulo `barberos` · creación del esquema
-- Dueño: backend/modulos/barberos. Solo este módulo escribe en el esquema `barberos`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

-- =====================================================================
-- BARBEROS (equipo, horarios, ausencias, bloqueos y documentos)
-- =====================================================================
CREATE SCHEMA barberos;

CREATE TYPE barberos.tipo_documento      AS ENUM ('CC', 'CE', 'PPT', 'PA');
CREATE TYPE barberos.tipo_vinculacion    AS ENUM ('prestacion_servicios', 'laboral_indefinido', 'laboral_termino_fijo');
CREATE TYPE barberos.periodo_liquidacion AS ENUM ('semanal', 'quincenal', 'mensual');
CREATE TYPE barberos.tipo_ausencia       AS ENUM ('vacaciones', 'incapacidad', 'licencia', 'suspension');
CREATE TYPE barberos.motivo_retiro       AS ENUM ('renuncia_voluntaria', 'terminacion_contrato', 'fin_prestacion_servicios', 'otro');
CREATE TYPE barberos.motivo_bloqueo      AS ENUM ('cita_medica', 'diligencia_personal', 'capacitacion', 'otro');
CREATE TYPE barberos.tipo_adjunto        AS ENUM ('identidad', 'contrato', 'hoja_de_vida', 'certificado', 'afiliacion', 'planilla_pila');
CREATE TYPE barberos.parentesco          AS ENUM ('madre_padre', 'pareja', 'hermano', 'hijo', 'otro');
CREATE TYPE barberos.medio_pago          AS ENUM ('transferencia', 'nequi', 'daviplata', 'efectivo');
CREATE TYPE barberos.campo_corregible    AS ENUM ('celular', 'correo', 'contacto_emergencia', 'medio_pago', 'otro');
CREATE TYPE barberos.estado_solicitud    AS ENUM ('pendiente', 'atendida', 'rechazada');
CREATE TYPE barberos.estado_barbero      AS ENUM ('activo', 'vacaciones', 'incapacidad_licencia', 'suspendido', 'inactivo');

-- Todo barbero tiene una cuenta (inactiva si no tiene acceso al panel);
-- su correo de contacto es el de la cuenta.
CREATE TABLE barberos.barbero (
  id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  usuario_id             bigint NOT NULL,
  rol_usuario            identidad.rol NOT NULL DEFAULT 'barbero' CHECK (rol_usuario = 'barbero'),
  tipo_documento         barberos.tipo_documento NOT NULL,
  numero_documento       text NOT NULL CHECK (numero_documento ~ '^[0-9A-Z]{4,20}$'),
  nombres                text NOT NULL CHECK (length(nombres) BETWEEN 2 AND 60),
  apellidos              text NOT NULL CHECK (length(apellidos) BETWEEN 2 AND 60),
  fecha_nacimiento       date NOT NULL,
  celular                nucleo.celular_co NOT NULL,
  especialidad           text NOT NULL CHECK (length(especialidad) BETWEEN 2 AND 60),
  anios_experiencia      smallint CHECK (anios_experiencia BETWEEN 0 AND 70),
  instagram              text CHECK (instagram ~ '^[A-Za-z0-9._]{1,30}$'),
  biografia              text CHECK (length(biografia) <= 160),
  foto_archivo_id        bigint REFERENCES archivos.archivo (id),
  fecha_ingreso          date NOT NULL,
  tipo_vinculacion       barberos.tipo_vinculacion NOT NULL,
  fin_contrato           date,
  porcentaje_comision    nucleo.porcentaje NOT NULL,
  periodo_liquidacion    barberos.periodo_liquidacion NOT NULL DEFAULT 'quincenal',
  mostrar_en_landing     boolean NOT NULL DEFAULT true,
  recibe_citas_en_linea  boolean NOT NULL DEFAULT true,
  orden                  smallint NOT NULL DEFAULT 0,      -- landing y «Cualquier barbero»
  fecha_retiro           date,
  motivo_retiro          barberos.motivo_retiro,
  creado_en              timestamptz NOT NULL DEFAULT now(),
  actualizado_en         timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT barbero_usuario_uk   UNIQUE (usuario_id),
  CONSTRAINT barbero_documento_uk UNIQUE (tipo_documento, numero_documento),
  CONSTRAINT barbero_usuario_fk   FOREIGN KEY (usuario_id, rol_usuario) REFERENCES identidad.usuario (id, rol),
  CONSTRAINT barbero_retiro_ck    CHECK ((fecha_retiro IS NULL) = (motivo_retiro IS NULL)),
  CONSTRAINT barbero_contrato_ck  CHECK (fin_contrato IS NULL OR fin_contrato >= fecha_ingreso)
);
CREATE INDEX barbero_vigente_orden_idx ON barberos.barbero (orden, id) WHERE fecha_retiro IS NULL;

-- Datos privados (solo administrador). Los campos *_cifrado se cifran en la
-- aplicación con AES-256-GCM antes de guardarse.
CREATE TABLE barberos.datos_privados (
  barbero_id               bigint PRIMARY KEY REFERENCES barberos.barbero (id) ON DELETE CASCADE,
  direccion                text,
  ciudad                   text,
  grupo_sanguineo_cifrado  bytea,
  emergencia_nombre        text,
  emergencia_parentesco    barberos.parentesco,
  emergencia_celular       nucleo.celular_co,
  eps                      text,
  fondo_pensiones          text,
  arl                      text,
  caja_compensacion        text,
  medio_pago               barberos.medio_pago,
  banco                    text,
  tipo_cuenta              text,
  numero_cuenta_cifrado    bytea,
  actualizado_en           timestamptz NOT NULL DEFAULT now()
);

-- Horario semanal. dia_semana sigue ISO 8601: 1 = lunes … 7 = domingo.
CREATE TABLE barberos.franja_horaria (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  barbero_id   bigint NOT NULL REFERENCES barberos.barbero (id) ON DELETE CASCADE,
  dia_semana   smallint NOT NULL CHECK (dia_semana BETWEEN 1 AND 7),
  hora_inicio  time NOT NULL,
  hora_fin     time NOT NULL,
  CHECK (hora_fin > hora_inicio),
  CONSTRAINT franja_sin_solape EXCLUDE USING gist (
    barbero_id WITH =,
    dia_semana WITH =,
    nucleo.rango_hora(hora_inicio, hora_fin) WITH &&
  )
);

CREATE TABLE barberos.ausencia (
  id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  barbero_id          bigint NOT NULL REFERENCES barberos.barbero (id),
  tipo                barberos.tipo_ausencia NOT NULL,
  desde               date NOT NULL,
  hasta               date NOT NULL,                   -- incluido
  observacion         text,
  soporte_archivo_id  bigint REFERENCES archivos.archivo (id),
  registrada_por      bigint NOT NULL REFERENCES identidad.usuario (id),
  registrada_en       timestamptz NOT NULL DEFAULT now(),
  anulada_en          timestamptz,
  CHECK (hasta >= desde),
  CONSTRAINT ausencia_sin_solape EXCLUDE USING gist (
    barbero_id WITH =,
    daterange(desde, hasta, '[]') WITH &&
  ) WHERE (anulada_en IS NULL)
);

CREATE TABLE barberos.bloqueo (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  barbero_id  bigint NOT NULL REFERENCES barberos.barbero (id),
  inicio      timestamptz NOT NULL,
  fin         timestamptz NOT NULL,
  motivo      barberos.motivo_bloqueo NOT NULL,
  nota        text CHECK (length(nota) <= 200),
  creado_por  bigint NOT NULL REFERENCES identidad.usuario (id),
  creado_en   timestamptz NOT NULL DEFAULT now(),
  CHECK (fin > inicio),
  CONSTRAINT bloqueo_sin_solape EXCLUDE USING gist (
    barbero_id WITH =,
    tstzrange(inicio, fin) WITH &&
  )
);

CREATE TABLE barberos.servicio_ofrecido (
  barbero_id   bigint NOT NULL REFERENCES barberos.barbero (id) ON DELETE CASCADE,
  servicio_id  bigint NOT NULL REFERENCES catalogo.servicio (id),
  PRIMARY KEY (barbero_id, servicio_id)
);
CREATE INDEX servicio_ofrecido_servicio_idx ON barberos.servicio_ofrecido (servicio_id);

CREATE TABLE barberos.documento (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  barbero_id   bigint NOT NULL REFERENCES barberos.barbero (id),
  tipo         barberos.tipo_adjunto NOT NULL,
  periodo      date,                                      -- mes de la planilla PILA (día 1)
  archivo_id   bigint NOT NULL REFERENCES archivos.archivo (id),
  cargado_por  bigint NOT NULL REFERENCES identidad.usuario (id),
  cargado_en   timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT documento_archivo_uk  UNIQUE (archivo_id),
  CONSTRAINT documento_periodo_ck  CHECK ((tipo = 'planilla_pila') = (periodo IS NOT NULL)),
  CONSTRAINT documento_periodo_dia_ck CHECK (periodo IS NULL OR extract(day FROM periodo) = 1)
);
CREATE INDEX documento_barbero_idx    ON barberos.documento (barbero_id, tipo);
CREATE UNIQUE INDEX documento_pila_uk ON barberos.documento (barbero_id, periodo) WHERE tipo = 'planilla_pila';

CREATE TABLE barberos.solicitud_correccion (
  id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  barbero_id       bigint NOT NULL REFERENCES barberos.barbero (id),
  campo            barberos.campo_corregible NOT NULL,
  valor_propuesto  text NOT NULL CHECK (length(valor_propuesto) BETWEEN 1 AND 500),
  estado           barberos.estado_solicitud NOT NULL DEFAULT 'pendiente',
  creada_en        timestamptz NOT NULL DEFAULT now(),
  atendida_por     bigint REFERENCES identidad.usuario (id),
  atendida_en      timestamptz,
  CHECK ((estado = 'pendiente') = (atendida_en IS NULL))
);
CREATE INDEX solicitud_correccion_pendiente_idx ON barberos.solicitud_correccion (creada_en) WHERE estado = 'pendiente';

-- Vista publicada: datos no sensibles del barbero (contrato de lectura).
CREATE VIEW barberos.v_barbero AS
SELECT id,
       nombres || ' ' || apellidos AS nombre,
       nombres                     AS nombre_corto,
       especialidad,
       orden,
       tipo_documento,
       numero_documento,
       fecha_ingreso,
       tipo_vinculacion,
       porcentaje_comision,
       fecha_retiro
FROM barberos.barbero;

-- Vista publicada: horario de atención dinámico (DA-10).
CREATE VIEW barberos.v_horario_atencion AS
SELECT f.dia_semana,
       min(f.hora_inicio) AS abre,
       max(f.hora_fin)    AS cierra
FROM barberos.franja_horaria f
JOIN barberos.barbero b ON b.id = f.barbero_id
WHERE b.fecha_retiro IS NULL
GROUP BY f.dia_semana;

-- Estado de cada barbero en una fecha (RN-24). La fecha la pasa la
-- aplicación desde su reloj, para que las pruebas puedan fijar el tiempo.
CREATE FUNCTION barberos.estados_en(p_fecha date)
RETURNS TABLE (barbero_id bigint, estado barberos.estado_barbero, ausencia_desde date, ausencia_hasta date)
LANGUAGE sql STABLE AS $$
  SELECT b.id,
         CASE
           WHEN b.fecha_retiro IS NOT NULL AND b.fecha_retiro <= p_fecha THEN 'inactivo'
           WHEN a.tipo = 'suspension'                  THEN 'suspendido'
           WHEN a.tipo IN ('incapacidad', 'licencia')  THEN 'incapacidad_licencia'
           WHEN a.tipo = 'vacaciones'                  THEN 'vacaciones'
           ELSE 'activo'
         END::barberos.estado_barbero,
         a.desde,
         a.hasta
  FROM barberos.barbero b
  LEFT JOIN barberos.ausencia a
         ON a.barbero_id = b.id
        AND a.anulada_en IS NULL
        AND p_fecha BETWEEN a.desde AND a.hasta
$$;

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON barberos.barbero
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON barberos.datos_privados
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();
