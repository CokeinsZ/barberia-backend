-- Módulo `catalogo` · creación del esquema
-- Dueño: backend/modulos/catalogo. Solo este módulo escribe en el esquema `catalogo`.
-- Generada a partir de docs/diseno/esquema.sql (versión consolidada de diseño).

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

-- Mantiene actualizado_en.
CREATE TRIGGER tocar_actualizado_en BEFORE UPDATE ON catalogo.servicio
  FOR EACH ROW EXECUTE FUNCTION nucleo.tocar_actualizado_en();

-- Datos iniciales.
INSERT INTO catalogo.categoria (nombre, orden) VALUES
  ('Cabello', 1), ('Barba', 2), ('Combos', 3), ('Complementos', 4);
