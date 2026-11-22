-- =====================================================================
-- Diamante Barbería · Esquema de base de datos
-- PostgreSQL 18
--
-- Un esquema de PostgreSQL por módulo del backend. Cada módulo es dueño
-- de sus tablas: solo él las escribe. Los demás módulos leen a través de
-- su API pública o de sus vistas publicadas (prefijo v_).
--
-- Orden: núcleo → identidad → archivos → negocio → catálogo → barberos →
-- clientes → verificación → notificaciones → agenda → ventas →
-- auditoría → trabajos → datos iniciales.
-- =====================================================================

BEGIN;

-- ---------------------------------------------------------------------
-- Extensiones
-- ---------------------------------------------------------------------
CREATE EXTENSION IF NOT EXISTS btree_gist;  -- restricciones de no solapamiento
CREATE EXTENSION IF NOT EXISTS citext;      -- correos sin distinguir mayúsculas
CREATE EXTENSION IF NOT EXISTS pg_trgm;     -- búsqueda aproximada por nombre
CREATE EXTENSION IF NOT EXISTS unaccent;    -- búsqueda sin tildes

-- =====================================================================
-- NÚCLEO (tipos compartidos; equivale al crate `nucleo`)
-- =====================================================================
CREATE SCHEMA nucleo;

CREATE DOMAIN nucleo.dinero     AS bigint        CHECK (VALUE >= 0);              -- pesos colombianos, sin decimales
CREATE DOMAIN nucleo.porcentaje AS numeric(5,2)  CHECK (VALUE BETWEEN 0 AND 100);
CREATE DOMAIN nucleo.celular_co AS text          CHECK (VALUE ~ '^3[0-9]{9}$');   -- celular colombiano sin +57
CREATE DOMAIN nucleo.correo     AS citext        CHECK (VALUE ~ '^[^@\s]+@[^@\s]+\.[^@\s]{2,}$');

CREATE TYPE nucleo.rango_hora AS RANGE (subtype = time);
CREATE TYPE nucleo.actor AS ENUM ('cliente', 'barbero', 'administrador', 'sistema');

-- Texto normalizado para búsquedas: minúsculas y sin tildes.
-- Se declara IMMUTABLE para poder indexarlo (patrón habitual con unaccent).
CREATE FUNCTION nucleo.normalizar(texto text) RETURNS text
  LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
  RETURN lower(public.unaccent('public.unaccent'::regdictionary, texto));

-- Mantiene la columna actualizado_en.
CREATE FUNCTION nucleo.tocar_actualizado_en() RETURNS trigger
  LANGUAGE plpgsql AS $$
BEGIN
  NEW.actualizado_en := now();
  RETURN NEW;
END $$;

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

-- =====================================================================
-- CATÁLOGO (servicios simples y combos)
-- =====================================================================
CREATE SCHEMA catalogo;

CREATE TYPE catalogo.tipo_servicio AS ENUM ('simple', 'combo');

CREATE TABLE catalogo.categoria (
  id      smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  nombre  text NOT NULL,
  orden   smallint NOT NULL DEFAULT 0,
  CONSTRAINT categoria_nombre_uk UNIQUE (nombre)
);

CREATE TABLE catalogo.servicio (
  id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  tipo                catalogo.tipo_servicio NOT NULL DEFAULT 'simple',
  categoria_id        smallint NOT NULL REFERENCES catalogo.categoria (id),
  nombre              text NOT NULL CHECK (length(nombre) BETWEEN 2 AND 80),
  descripcion         text NOT NULL CHECK (length(descripcion) BETWEEN 1 AND 300),
  precio              nucleo.dinero NOT NULL CHECK (precio > 0),
  duracion_min        smallint CHECK (duracion_min BETWEEN 5 AND 480),
  agendable_en_linea  boolean NOT NULL DEFAULT true,
  visible_en_landing  boolean NOT NULL DEFAULT true,
  activo              boolean NOT NULL DEFAULT true,
  destacado           boolean NOT NULL DEFAULT false,
  etiqueta            text CHECK (length(etiqueta) <= 30),
  icono               text,
  orden               smallint NOT NULL DEFAULT 0,
  creado_en           timestamptz NOT NULL DEFAULT now(),
  actualizado_en      timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT servicio_id_tipo_uk UNIQUE (id, tipo),
  CONSTRAINT servicio_duracion_ck CHECK (NOT agendable_en_linea OR duracion_min IS NOT NULL),
  CONSTRAINT servicio_combo_duracion_ck CHECK (tipo = 'simple' OR duracion_min IS NOT NULL)
);
CREATE UNIQUE INDEX servicio_nombre_uk   ON catalogo.servicio (nucleo.normalizar(nombre));
CREATE INDEX servicio_categoria_idx      ON catalogo.servicio (categoria_id);
CREATE INDEX servicio_landing_idx        ON catalogo.servicio (orden) WHERE activo AND visible_en_landing;

-- Composición de un combo. Las llaves compuestas garantizan en la base de
-- datos que un combo solo agrupa servicios simples.
CREATE TABLE catalogo.componente_combo (
  combo_id         bigint NOT NULL,
  tipo_combo       catalogo.tipo_servicio NOT NULL DEFAULT 'combo'  CHECK (tipo_combo = 'combo'),
  servicio_id      bigint NOT NULL,
  tipo_componente  catalogo.tipo_servicio NOT NULL DEFAULT 'simple' CHECK (tipo_componente = 'simple'),
  PRIMARY KEY (combo_id, servicio_id),
  CONSTRAINT componente_combo_fk     FOREIGN KEY (combo_id, tipo_combo)          REFERENCES catalogo.servicio (id, tipo) ON DELETE CASCADE,
  CONSTRAINT componente_servicio_fk  FOREIGN KEY (servicio_id, tipo_componente)  REFERENCES catalogo.servicio (id, tipo)
);
CREATE INDEX componente_combo_servicio_idx ON catalogo.componente_combo (servicio_id);

-- Vista publicada (contrato de lectura).
CREATE VIEW catalogo.v_servicio AS
SELECT s.id, s.tipo, s.nombre, c.nombre AS categoria, s.precio, s.duracion_min, s.activo
FROM catalogo.servicio s
JOIN catalogo.categoria c ON c.id = s.categoria_id;

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

-- =====================================================================
-- AGENDA (citas, configuración de agendamiento y recordatorios)
-- =====================================================================
CREATE SCHEMA agenda;

CREATE TYPE agenda.estado_cita            AS ENUM ('sin_confirmar', 'confirmada', 'en_curso', 'atendida', 'no_asistio', 'cancelada');
CREATE TYPE agenda.origen_cita            AS ENUM ('en_linea', 'panel');
CREATE TYPE agenda.motivo_cancelacion     AS ENUM ('solicitud_cliente', 'barbero_no_disponible', 'otro');
CREATE TYPE agenda.evento_cita            AS ENUM ('creada', 'confirmada', 'iniciada', 'atendida', 'no_asistio',
                                                   'cancelada', 'reprogramada', 'reasignada');
CREATE TYPE agenda.proposito_mensaje      AS ENUM ('confirmacion', 'recordatorio_24h', 'recordatorio_2h', 'cancelacion', 'reprogramacion',
                                                   'recordatorio_manual', 'aviso_reprogramacion');
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

-- =====================================================================
-- VENTAS (ventas, pagos, descuentos y cierres de caja)
-- =====================================================================
CREATE SCHEMA ventas;

CREATE TYPE ventas.tipo_descuento AS ENUM ('porcentaje', 'valor_fijo');
CREATE TYPE ventas.estado_venta   AS ENUM ('registrada', 'anulada');

CREATE TABLE ventas.metodo_pago (
  id      smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  codigo  text NOT NULL CHECK (codigo ~ '^[a-z_]{2,30}$'),
  nombre  text NOT NULL,
  activo  boolean NOT NULL DEFAULT true,
  orden   smallint NOT NULL DEFAULT 0,
  CONSTRAINT metodo_pago_codigo_uk UNIQUE (codigo)
);

CREATE TABLE ventas.descuento (
  id                    smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  nombre                text NOT NULL,
  tipo                  ventas.tipo_descuento NOT NULL,
  valor                 numeric(12,2) NOT NULL CHECK (valor > 0),
  permitido_a_barberos  boolean NOT NULL DEFAULT false,
  activo                boolean NOT NULL DEFAULT true,
  orden                 smallint NOT NULL DEFAULT 0,
  CONSTRAINT descuento_nombre_uk UNIQUE (nombre),
  CHECK (tipo <> 'porcentaje' OR valor <= 100)
);

CREATE TABLE ventas.motivo_descuento (
  id      smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  nombre  text NOT NULL,
  activo  boolean NOT NULL DEFAULT true,
  orden   smallint NOT NULL DEFAULT 0,
  CONSTRAINT motivo_descuento_nombre_uk UNIQUE (nombre)
);

-- Consecutivo sin huecos: se incrementa con bloqueo de fila dentro de la
-- transacción de la venta (una secuencia dejaría huecos al revertir).
CREATE TABLE ventas.contador_ticket (
  id      smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  ultimo  bigint NOT NULL DEFAULT 0 CHECK (ultimo >= 0)
);

CREATE TABLE ventas.venta (
  id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  numero_ticket        bigint NOT NULL,
  registrada_en        timestamptz NOT NULL DEFAULT now(),
  fecha_operacion      date GENERATED ALWAYS AS ((registrada_en AT TIME ZONE 'America/Bogota')::date) STORED,
  registrada_por       bigint NOT NULL REFERENCES identidad.usuario (id),
  barbero_id           bigint NOT NULL REFERENCES barberos.barbero (id),
  cliente_id           bigint REFERENCES clientes.cliente (id),   -- NULL = cliente ocasional
  cita_id              bigint REFERENCES agenda.cita (id),
  descuento_id         smallint REFERENCES ventas.descuento (id),
  motivo_descuento_id  smallint REFERENCES ventas.motivo_descuento (id),
  descuento_valor      nucleo.dinero NOT NULL DEFAULT 0,          -- valor aplicado (hecho histórico)
  propina              nucleo.dinero NOT NULL DEFAULT 0,
  porcentaje_comision  nucleo.porcentaje NOT NULL,                -- copia del % vigente del barbero
  estado               ventas.estado_venta NOT NULL DEFAULT 'registrada',
  anulada_en           timestamptz,
  anulada_por          bigint REFERENCES identidad.usuario (id),
  motivo_anulacion     text,
  CONSTRAINT venta_ticket_uk UNIQUE (numero_ticket),
  CONSTRAINT venta_cita_uk   UNIQUE (cita_id),
  CONSTRAINT venta_descuento_ck CHECK (
    (descuento_id IS NULL) = (motivo_descuento_id IS NULL)
    AND (descuento_id IS NULL) = (descuento_valor = 0)
  ),
  CONSTRAINT venta_anulacion_ck CHECK (
    (estado = 'anulada') = (anulada_en IS NOT NULL)
    AND (anulada_en IS NULL) = (anulada_por IS NULL)
    AND (anulada_en IS NULL) = (motivo_anulacion IS NULL)
  )
);
CREATE INDEX venta_fecha_idx         ON ventas.venta (fecha_operacion) WHERE estado = 'registrada';
CREATE INDEX venta_barbero_fecha_idx ON ventas.venta (barbero_id, fecha_operacion) WHERE estado = 'registrada';
CREATE INDEX venta_cliente_fecha_idx ON ventas.venta (cliente_id, fecha_operacion DESC) WHERE cliente_id IS NOT NULL;

CREATE TABLE ventas.venta_linea (
  venta_id     bigint NOT NULL REFERENCES ventas.venta (id) ON DELETE CASCADE,
  posicion     smallint NOT NULL CHECK (posicion BETWEEN 1 AND 20),
  servicio_id  bigint NOT NULL REFERENCES catalogo.servicio (id),
  precio       nucleo.dinero NOT NULL,                     -- precio vigente al vender
  PRIMARY KEY (venta_id, posicion)
);
CREATE INDEX venta_linea_servicio_idx ON ventas.venta_linea (servicio_id);

-- Una venta se puede pagar con uno o más métodos (el MVP usa uno).
CREATE TABLE ventas.venta_pago (
  venta_id        bigint NOT NULL REFERENCES ventas.venta (id) ON DELETE CASCADE,
  metodo_pago_id  smallint NOT NULL REFERENCES ventas.metodo_pago (id),
  monto           nucleo.dinero NOT NULL CHECK (monto > 0),
  PRIMARY KEY (venta_id, metodo_pago_id)
);
CREATE INDEX venta_pago_metodo_idx ON ventas.venta_pago (metodo_pago_id);

-- Cuadre de la venta, verificado al confirmar la transacción:
-- al menos una línea; descuento ≤ subtotal; pagos = subtotal − descuento + propina.
CREATE FUNCTION ventas.verificar_cuadre() RETURNS trigger
  LANGUAGE plpgsql AS $$
DECLARE
  v_id        bigint;
  v_desc      bigint;
  v_propina   bigint;
  v_lineas    integer;
  v_subtotal  bigint;
  v_pagos     bigint;
BEGIN
  -- Cada rama se resuelve por separado: NEW.venta_id no existe en la tabla venta.
  IF TG_TABLE_NAME = 'venta' THEN
    v_id := NEW.id;
  ELSIF TG_OP = 'DELETE' THEN
    v_id := OLD.venta_id;
  ELSE
    v_id := NEW.venta_id;
  END IF;

  SELECT descuento_valor, propina INTO v_desc, v_propina FROM ventas.venta WHERE id = v_id;
  IF NOT FOUND THEN
    RETURN NULL;   -- la venta ya no existe (borrado en cascada)
  END IF;
  SELECT count(*), coalesce(sum(precio), 0) INTO v_lineas, v_subtotal FROM ventas.venta_linea WHERE venta_id = v_id;
  SELECT coalesce(sum(monto), 0) INTO v_pagos FROM ventas.venta_pago WHERE venta_id = v_id;
  IF v_lineas = 0 THEN
    RAISE EXCEPTION 'La venta % no tiene servicios', v_id USING ERRCODE = 'check_violation';
  END IF;
  IF v_desc > v_subtotal THEN
    RAISE EXCEPTION 'El descuento de la venta % supera el subtotal', v_id USING ERRCODE = 'check_violation';
  END IF;
  IF v_pagos <> v_subtotal - v_desc + v_propina THEN
    RAISE EXCEPTION 'Los pagos de la venta % (%) no cuadran con el total (%)', v_id, v_pagos, v_subtotal - v_desc + v_propina
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER venta_cuadre_trg AFTER INSERT OR UPDATE ON ventas.venta
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();
CREATE CONSTRAINT TRIGGER venta_linea_cuadre_trg AFTER INSERT OR UPDATE OR DELETE ON ventas.venta_linea
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();
CREATE CONSTRAINT TRIGGER venta_pago_cuadre_trg AFTER INSERT OR UPDATE OR DELETE ON ventas.venta_pago
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ventas.verificar_cuadre();

CREATE TABLE ventas.cierre_caja (
  id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  fecha_operacion  date NOT NULL,
  cerrado_por      bigint NOT NULL REFERENCES identidad.usuario (id),
  cerrado_en       timestamptz NOT NULL DEFAULT now(),
  observacion      text,
  CONSTRAINT cierre_caja_fecha_uk UNIQUE (fecha_operacion)
);

-- Lo esperado se guarda como foto del momento del cierre; la diferencia
-- (contado − esperado) se calcula al consultar.
CREATE TABLE ventas.cierre_caja_detalle (
  cierre_id       bigint NOT NULL REFERENCES ventas.cierre_caja (id) ON DELETE CASCADE,
  metodo_pago_id  smallint NOT NULL REFERENCES ventas.metodo_pago (id),
  esperado        nucleo.dinero NOT NULL,
  contado         nucleo.dinero,                         -- NULL = no verificado
  PRIMARY KEY (cierre_id, metodo_pago_id)
);

-- Vista publicada: totales de cada venta vigente (contrato de lectura de
-- reportes y del dashboard).
CREATE VIEW ventas.v_venta AS
SELECT v.id,
       v.numero_ticket,
       v.fecha_operacion,
       v.registrada_en,
       v.barbero_id,
       v.cliente_id,
       v.cita_id,
       l.subtotal,
       v.descuento_valor,
       l.subtotal - v.descuento_valor                                          AS venta_neta,
       v.propina,
       l.subtotal - v.descuento_valor + v.propina                              AS total_cobrado,
       v.porcentaje_comision,
       round((l.subtotal - v.descuento_valor) * v.porcentaje_comision / 100)::bigint AS comision
FROM ventas.venta v
CROSS JOIN LATERAL (
  SELECT sum(vl.precio)::bigint AS subtotal FROM ventas.venta_linea vl WHERE vl.venta_id = v.id
) l
WHERE v.estado = 'registrada';

-- Vistas publicadas para `reportes`: servicios vendidos y lo cobrado por método (solo ventas
-- vigentes).
CREATE VIEW ventas.v_venta_linea AS
SELECT vl.venta_id,
       v.fecha_operacion,
       v.registrada_en,
       v.barbero_id,
       v.cliente_id,
       vl.posicion,
       vl.servicio_id,
       vl.precio
FROM ventas.venta_linea vl
JOIN ventas.venta v ON v.id = vl.venta_id
WHERE v.estado = 'registrada';

CREATE VIEW ventas.v_venta_pago AS
SELECT p.venta_id,
       v.fecha_operacion,
       v.barbero_id,
       m.codigo AS metodo,
       m.nombre AS metodo_nombre,
       p.monto
FROM ventas.venta_pago p
JOIN ventas.venta v ON v.id = p.venta_id
JOIN ventas.metodo_pago m ON m.id = p.metodo_pago_id
WHERE v.estado = 'registrada';

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

-- =====================================================================
-- Triggers de actualizado_en
-- =====================================================================
DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY[
    'identidad.usuario', 'negocio.negocio', 'catalogo.servicio', 'barberos.barbero',
    'barberos.datos_privados', 'clientes.cliente', 'clientes.configuracion_segmentos',
    'notificaciones.mensaje', 'agenda.configuracion', 'agenda.cita'
  ] LOOP
    EXECUTE format('CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON %s
                    FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en()', t);
  END LOOP;
END $$;

-- =====================================================================
-- Datos iniciales
-- =====================================================================
INSERT INTO negocio.negocio (nombre_comercial) VALUES ('Diamante Barbería');
INSERT INTO agenda.configuracion DEFAULT VALUES;
INSERT INTO clientes.configuracion_segmentos DEFAULT VALUES;
INSERT INTO ventas.contador_ticket DEFAULT VALUES;

INSERT INTO catalogo.categoria (nombre, orden) VALUES
  ('Cabello', 1), ('Barba', 2), ('Combos', 3), ('Complementos', 4);

INSERT INTO ventas.metodo_pago (codigo, nombre, activo, orden) VALUES
  ('efectivo', 'Efectivo', true, 1),
  ('nequi', 'Nequi', true, 2),
  ('daviplata', 'Daviplata', true, 3),
  ('tarjeta', 'Tarjeta', true, 4),
  ('transferencia', 'Transferencia', true, 5),
  ('link_pago', 'Link de pago', false, 6);

INSERT INTO ventas.descuento (nombre, tipo, valor, permitido_a_barberos, orden) VALUES
  ('10 %', 'porcentaje', 10, true, 1),
  ('15 %', 'porcentaje', 15, true, 2),
  ('$5.000', 'valor_fijo', 5000, true, 3);

INSERT INTO ventas.motivo_descuento (nombre, orden) VALUES
  ('Cliente frecuente', 1), ('Promoción vigente', 2), ('Cortesía de la casa', 3), ('Otro motivo', 4);

COMMIT;
