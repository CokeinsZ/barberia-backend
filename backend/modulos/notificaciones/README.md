# Módulo `notificaciones`

Cola de salida de mensajes (patrón *outbox*), correo SMTP y, en la fase 4, el envío por
WhatsApp Cloud API, webhooks y avisos internos.

| | |
|---|---|
| Fase del plan | 1 (correo) ✅ · 3 (WhatsApp en cola) ✅ · 4 (envío por WhatsApp, webhooks, avisos) |
| Esquema de base de datos | `notificaciones` (ver `migraciones/`) |
| Fachada | `api::BuzonSalida<C>::encolar_correo` / `encolar_whatsapp` y las plantillas `api::PlantillaCorreo` / `api::PlantillaWhatsApp` |

## Cómo funciona el correo

1. Un módulo encola con `BuzonSalida::encolar_correo(&mut cx, …)` **dentro de su
   transacción**: si la operación se revierte, el correo no existe; si se confirma, no se pierde.
2. El worker (`diamante worker`) llama cada 5 segundos a `enviar_correos_pendientes`: reserva un
   lote con `FOR UPDATE SKIP LOCKED` moviendo `programado_para` 5 minutos adelante (si el
   worker muere, el mensaje vuelve a estar disponible), lo envía por SMTP y marca el resultado.
3. Fallos temporales: reintento con espera exponencial (1, 2, 4, 8 minutos). Al 5.º intento, o
   ante un fallo permanente (dirección inválida, buzón inexistente), queda `fallido`.
4. Las plantillas con enlaces secretos (`contiene_secreto`) borran sus variables al enviarse o
   al fallar definitivamente: el token no queda guardado.

Sin `DIAMANTE_SMTP_HOST` los correos quedan en cola sin enviarse. En desarrollo y E2E, Mailpit
los atrapa: <http://localhost:8025>.

| Plantilla | Uso |
|---|---|
| `invitacion_personal` | Crear la contraseña de una cuenta nueva del personal (vence en 7 días) |
| `recuperacion_contrasena` | Restablecer la contraseña (vence en 1 hora) |

## WhatsApp (fase 3: en cola)

`encolar_whatsapp` guarda el mensaje con canal `whatsapp` en la misma transacción del caso de
uso; el envío real llega en la fase 4. Mientras tanto, con `DIAMANTE_PRUEBAS_E2E=true` (nunca en
producción: la API no arranca) los mensajes recientes de un celular se leen en
`GET /api/v1/_pruebas/mensajes?destinatario=3001112233` (los últimos 20).

| Plantilla | Uso | Secreto |
|---|---|---|
| `codigo_verificacion` | Código para reservar | Sí |
| `cita_confirmada` | Confirmación con enlace de gestión | Sí |
| `cita_reprogramada` | Aviso con el enlace nuevo | Sí |
| `cita_cancelada` | Aviso de cancelación | No |

## Eventos que publica (crate `contratos`)

- `RespuestaWhatsAppRecibida` (fase 4)

## Estructura

```text
notificaciones/src/
├── api.rs                       # BuzonSalida<C>, CorreoSaliente, PlantillaCorreo, ResumenEnvio
├── modulo.rs                    # buzon() y enviar_correos_pendientes()
├── dominio/                     # plantillas (texto y HTML escapado) y política de reintentos
├── aplicacion/                  # EnviarCorreosPendientes y sus puertos
└── infraestructura/
    ├── postgres/                # BuzonPg (outbox) y la cola con SKIP LOCKED
    └── smtp/                    # lettre: STARTTLS, TLS o sin cifrado (solo desarrollo)
tests/cola_postgres.rs           # transacción revertida = sin correo; reintento tras un fallo
```
