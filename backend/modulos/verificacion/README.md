# Módulo `verificacion`

Códigos de verificación por WhatsApp y comprobantes de un solo uso para reservar en línea
(RF-RES-08, RF-RES-09, RNF-SEG-05, RNF-SEG-06).

| | |
|---|---|
| Fase del plan | 3 ✅ (WhatsApp simulado: el mensaje queda en cola) · 4 (envío real) |
| Esquema de base de datos | `verificacion` (ver `migraciones/`) |
| Fachada | `api::Comprobantes<C>::consumir` (la usa `agenda` en la transacción de la reserva) |
| Puerto que implementa otro módulo | `api::ClientesConocidos<C>` (lo implementa `app` con la fachada de `clientes`) |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| POST | `/publico/verificaciones` | Público | `{ celular }` → `202 { id, expira_en, reenviar_desde }`. Nunca revela si el cliente existe |
| POST | `/publico/verificaciones/{id}/confirmar` | Público | `{ codigo }` → `{ comprobante, cliente }`; `cliente` trae nombre, correo y nacimiento si ya existe (E2E-RES-02) |

## Reglas

- Código de 4 dígitos (muestreo por rechazo, sin sesgo), válido **10 minutos**, guardado como
  HMAC-SHA256 con la clave del servidor; se compara en tiempo constante.
- **5 intentos**: cada fallo responde `422 codigo_incorrecto` con `restantes`; al 5.º el código
  queda invalidado (`410 codigo_vencido`) y hay que pedir otro.
- Reenvío: **60 segundos** de espera (`429` con `Retry-After`).
- Límites en memoria: 3 códigos por celular cada 15 minutos y 10 por IP cada hora.
- El comprobante (256 bits, guardado como SHA-256) vale **30 minutos** y autoriza **una** cita.
  Si la reserva falla (p. ej., `409 conflicto_horario`), la transacción se revierte y el mismo
  comprobante sirve para elegir otra hora (E2E-RES-07).

## WhatsApp simulado (desarrollo y E2E)

Hasta la fase 4 los mensajes quedan en `notificaciones.mensaje`. Con `DIAMANTE_PRUEBAS_E2E=true`
(solo fuera de producción) se leen en `GET /api/v1/_pruebas/mensajes?destinatario=3001112233`.

## Eventos

- Publica: ninguno. Consume: ninguno.

## Estructura

```text
verificacion/src/
├── api.rs                 # Comprobantes<C>, ClientesConocidos<C>, DTO y ErrorVerificacion
├── modulo.rs              # rutas() y comprobantes()
├── dominio/               # vigencias, generación del código, mensaje y comparación constante
├── aplicacion/            # Verificacion<C>: solicitar, confirmar y consumir
├── infraestructura/       # RepositorioCodigosPg
└── http/                  # rutas públicas y límites de intentos
```
