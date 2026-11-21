# Módulo `catalogo`

Servicios simples y combos con su categoría, precio y duración (RF-SER-01 a 05 y 08).

| | |
|---|---|
| Fase del plan | 2 ✅ |
| Esquema de base de datos | `catalogo` (ver `migraciones/`) |
| Fachada | `api::CatalogoServicios<C>::resumenes` (la usan `barberos` y, más adelante, `agenda` y `ventas`) |
| Puerto que implementa otro módulo | `api::OferentesServicio<C>` (lo implementa `barberos`: cuántos barberos vigentes ofrecen cada servicio) |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/categorias` | Personal | Cabello, Barba, Combos y Complementos |
| GET | `/servicios?incluir_inactivos=` | Personal | Con componentes, barberos que lo ofrecen y advertencia si ninguno |
| GET | `/servicios/{id}` | Personal | |
| POST | `/servicios` | Administrador | `tipo` `simple` (por defecto) o `combo` con `componentes` |
| PUT | `/servicios/{id}` | Administrador | El tipo no cambia; un precio nuevo solo afecta lo nuevo (RF-SER-03) |
| PATCH | `/servicios/{id}/estado` | Administrador | `{ activo }` (RF-SER-02) |
| PUT | `/servicios/{id}/componentes` | Administrador | `{ servicio_ids }`: dos o más simples activos |

## Reglas

- Nombre único sin distinguir mayúsculas ni tildes (índice sobre `nucleo.normalizar`).
- Duración de 5 a 480 minutos en múltiplos de 5; obligatoria si se agenda en línea y en los combos.
- Un combo agrupa dos o más servicios **simples** distintos y activos; la base lo garantiza con
  llaves foráneas compuestas `(id, tipo)`.
- La etiqueta solo aplica a los destacados (RF-SER-08).
- Todos los cambios se auditan con el antes y el después.
