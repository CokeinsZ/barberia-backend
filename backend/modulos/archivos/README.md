# Módulo `archivos`

Fotos y documentos: contenido en el volumen `archivos` (`DIAMANTE_ARCHIVOS_DIR`) y metadatos
en `archivos.archivo` (RNF-SEG-09).

| | |
|---|---|
| Fase del plan | 1 ✅ |
| Fachada | `api::Archivos<C>`: `guardar`, `eliminar` + `borrar_contenido`, `metadatos`, `urls_publicas`, `url_firmada` |
| Endpoints | `GET /api/v1/archivos/{id}?exp=&firma=` (descarga privada firmada) y `GET /media/publico/…` (fotos, con caché de un año) |
| Configuración | `DIAMANTE_ARCHIVOS_DIR`, `DIAMANTE_CLAVE_FIRMA` (obligatoria en producción) |

## Reglas

- **El tipo se detecta por el contenido** (firmas de JPEG, PNG, WebP y PDF), nunca por la
  extensión ni por el `Content-Type` del navegador. Cada módulo dueño fija los tipos, el tamaño
  máximo y si la imagen debe ser vertical.
- **Claves aleatorias** (128 bits) con la forma `publico|privado/<carpeta>/<aleatorio>.<ext>`;
  una clave con `..`, rutas absolutas o caracteres raros se rechaza.
- **Escritura atómica** (temporal + `rename`). Los metadatos se registran en la transacción de
  quien llama; si esa transacción se revierte, el contenido queda huérfano para la limpieza
  (fase 4). Al eliminar, el registro se borra en la transacción y el contenido **después** de
  confirmar (`borrar_contenido`).
- **Privados**: solo con una URL firmada (HMAC-SHA256 de `archivo:{id}:{vence}`) que vence en
  minutos; la respuesta es siempre una descarga (`attachment`, `nosniff`, `no-store`).
- **Públicos**: claves inmutables, caché de un año.

## Estructura

```text
archivos/src/
├── api.rs                 # fachada Archivos<C>, ArchivoNuevo, ErrorArchivos
├── dominio/               # TipoArchivo (firmas mágicas), ClaveArchivo, nombres saneados
├── aplicacion/            # ServicioArchivos (fachada y descargas) y sus puertos
├── infraestructura/       # disco (volumen) y PostgreSQL
└── http/                  # descarga firmada y /media
tests/archivos_postgres.rs # PostgreSQL y directorio reales, URL firmadas y /media
```
