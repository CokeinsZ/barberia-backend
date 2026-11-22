# ADR 0006 — Archivos en un volumen con URL firmadas

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

La plataforma guarda fotos de barberos (públicas: las muestra la landing) y documentos con
datos personales (cédula, contrato, incapacidades, planillas: privados). RNF-SEG-09 exige validar
tipo y tamaño, guardar los documentos en almacenamiento privado y servirlos con URL firmadas de
corta duración. El despliegue es un solo VPS con Docker Compose.

## Decisión

- **Contenido en un volumen de Docker** (`archivos`, montado en `/datos/archivos`), con dos
  carpetas: `publico/` y `privado/`. **Metadatos en PostgreSQL** (`archivos.archivo`), con el
  SHA-256 del contenido.
- **Fotos públicas**: la API sirve `/media/publico/*` desde el volumen (en un principio lo iba a
  hacer Caddy; ver ADR 0007). Las claves son aleatorias (128 bits) e inmutables: caché de un año.
- **Documentos privados**: solo con `GET /api/v1/archivos/{id}?exp=…&firma=…`, donde la firma es
  HMAC-SHA256 de `archivo:{id}:{vence}` con `DIAMANTE_CLAVE_FIRMA`. Las rutas del panel que listan
  documentos devuelven URL que vencen en 5 minutos.
- **Validación por contenido** (firmas mágicas de JPEG, PNG, WebP y PDF) y tamaño máximo por
  uso; nunca se confía en la extensión ni en el `Content-Type` del navegador.
- **Consistencia**: el contenido se escribe (atómicamente) antes de registrar los metadatos en la
  transacción del módulo dueño; al borrar, el registro sale en la transacción y el contenido
  después de confirmar. Lo que quede huérfano lo limpia una tarea de mantenimiento (fase 4).
- El almacenamiento está detrás del puerto `Almacenamiento`: cambiar a S3 o similar es otra
  implementación, sin tocar los casos de uso.

## Consecuencias

- Sin servicios externos ni costos adicionales; el respaldo diario incluye el volumen.
- Una URL firmada filtrada sirve solo unos minutos; la respuesta es siempre un adjunto con
  `nosniff`, así un archivo malicioso no se ejecuta en el sitio.
- Si se escala a varias instancias de la API, el volumen debe ser compartido (o pasar a S3).
