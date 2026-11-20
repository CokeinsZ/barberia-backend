-- Base compartida: extensiones y esquema `nucleo` (dominios, tipos y funciones).
-- Dueño: backend/crates/plataforma. Se aplica antes que cualquier módulo.

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
