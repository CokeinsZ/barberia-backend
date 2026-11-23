# Módulo `notificaciones`

Cola de salida de mensajes (patrón *outbox*), correo SMTP, WhatsApp (WhatsApp Cloud API de Meta
o simulado fuera de producción), webhooks firmados y avisos internos para la administradora.

| | |
|---|---|
| Fase del plan | 1 (correo) ✅ · 4 (WhatsApp, webhooks y avisos) ✅ · 8 (envío real por Meta, vencimiento en la cola, vigilancia) ✅ |
| Esquema de base de datos | `notificaciones` (ver `migraciones/`) |
| Fachadas | `api::BuzonSalida<C>` (encolar correo y WhatsApp), `api::Avisos<C>` (avisos internos) y `api::ConsultaMensajes<C>` (estado de mensajes concretos), todas en la transacción de quien llama |
| Plantillas de WhatsApp | `docs/diseno/plantillas-whatsapp.md`: el texto registrado en Meta (`es_CO`), sus variables en orden y los botones (una prueba verifica que coincide con el código) |
| Puerto que necesita | `api::NombreDelNegocio`: el nombre comercial para `{{2}}` de las plantillas de citas (lo implementa `app` con la fachada de `negocio`) |

## Cómo funciona la cola

1. Un módulo encola con `BuzonSalida` **dentro de su transacción**: si la operación se revierte,
   el mensaje no existe; si se confirma, no se pierde (RNF-DIS-03).
2. El worker (cada 5 s) reserva un lote por canal con `FOR UPDATE SKIP LOCKED` moviendo
   `programado_para` 5 minutos adelante (si el worker muere, el mensaje vuelve a la cola).
3. Fallos temporales: reintento a 1, 2, 4 y 8 minutos. Al 5.º intento, o ante un fallo
   permanente, queda `fallido`; si era un mensaje de una cita (confirmación, recordatorio,
   cambio, cancelación o aviso de reprogramación), la administradora recibe un **aviso**
   (RF-NOT-07, E2E-NOT-04).
4. Las plantillas con enlaces o códigos (`contiene_secreto`) borran sus variables al enviarse o
   al fallar definitivamente.
5. **Vencimiento**: un WhatsApp que esperó en la cola más de 24 horas (10 minutos un código de
   verificación) ya no se envía: queda `cancelado` con el motivo. Así un atraso largo, o
   conectar Meta sobre una cola vieja, no manda mensajes viejos.

| Canal | Sin configuración | Con configuración |
|---|---|---|
| Correo | En cola, sin enviarse | SMTP (`DIAMANTE_SMTP_*`); Mailpit en desarrollo y E2E |
| WhatsApp | Fuera de producción: **simulado**. En producción: en cola | WhatsApp Cloud API con `DIAMANTE_WHATSAPP_TOKEN` y `DIAMANTE_WHATSAPP_ID_NUMERO` (idioma `DIAMANTE_WHATSAPP_IDIOMA`, `es_CO`; versión `DIAMANTE_WHATSAPP_API`, v26.0) |

**WhatsApp Cloud API** (`infraestructura/whatsapp_meta.rs`): `POST {api}/{id_numero}/messages`
con el token como `Bearer`, la plantilla por nombre e idioma, los parámetros del cuerpo en orden,
el código también en el botón «Copiar código» y la carga (`confirmar`, `cancelar`) en los botones
del recordatorio. El celular va con el indicativo (`57…`). El `wamid` que responde Meta queda en el
mensaje y une los webhooks de estado y las respuestas. Los errores se clasifican: límites de
envío, fallas de Meta y credenciales se reintentan; un número sin WhatsApp (131026) o una
plantilla que no coincide (132xxx) quedan fallidos de inmediato, con aviso si era de una cita.
Probado contra un Meta de mentira (un servidor HTTP local): `infraestructura/whatsapp_meta.rs` y
`app/src/pruebas_whatsapp.rs` (confirmación, entrega, recordatorio con botones y «Confirmo»,
código y número sin WhatsApp).

### WhatsApp simulado

No llama a Meta: marca el mensaje como enviado con un id `wamid.simulado.…` y escribe en el log
el texto exacto de la plantilla. **Conserva las variables** (los mensajes nunca salen del
sistema), así las pruebas leen los códigos con `GET /api/v1/_pruebas/mensajes`. Para probar
fallas: los celulares terminados en `0000` fallan temporalmente y los terminados en `9999`, de
forma permanente.

## Webhooks de WhatsApp

- `GET /api/v1/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=…&hub.challenge=…`:
  registro en Meta; devuelve el reto solo con `DIAMANTE_WHATSAPP_TOKEN_VERIFICACION`.
- `POST /api/v1/webhooks/whatsapp`: verifica `X-Hub-Signature-256` (HMAC-SHA256 del cuerpo
  exacto con `DIAMANTE_WHATSAPP_SECRETO_APP`; `401 firma_invalida` si no coincide, sin guardar
  nada: RNF-SEG-10), guarda el evento y responde `200`. Si un texto del evento trae el carácter
  nulo (`jsonb` no lo admite), se guarda con `�` en su lugar: un error haría que Meta lo
  reintentara por días. El worker lo procesa después, cada evento en su propia transacción
  (si uno falla, queda marcado con su error y no se reintenta en bucle):
  - **Estados de entrega** (`sent`, `delivered`, `read`, `failed`): solo avanzan (pueden llegar
    desordenados); un `failed` de un mensaje de cita crea un aviso.
  - **Botones** («Confirmo», «Cancelar cita»): publica `RespuestaWhatsAppRecibida` con el id del
    mensaje propio; la agenda confirma o cancela la cita en la misma transacción.

Sin los dos secretos configurados, el webhook responde 404.

## Endpoints del panel (`/api/v1`, solo administradores)

| Método | Ruta | Notas |
|---|---|---|
| GET | `/avisos?no_leidos=true&pagina=` | Avisos internos: mensajes fallidos y citas afectadas por cambios de horario, bloqueos o ausencias |
| POST | `/avisos/{id}/leido` | Idempotente: conserva la primera lectura |
| GET | `/mensajes?destinatario=&estado=&canal=&plantilla=&pagina=` | Registro de mensajes con su estado, sin variables (RF-NOT-07) |

**Vigilancia** (RNF-OBS-02): `ModuloNotificaciones::estado_cola()` da, por canal, si está
configurado, los pendientes cuya hora ya llegó, el atraso del más antiguo y los enviados y
fallidos (24 h y última hora), y el estado de los webhooks. Lo usan `GET /api/salud/operacion` y
`GET /api/v1/sistema/estado` (en `app`).

| Plantilla de correo | Uso |
|---|---|
| `invitacion_personal` | Crear la contraseña de una cuenta del personal (vence en 7 días) |
| `recuperacion_contrasena` | Restablecer la contraseña (vence en 1 hora) |

## Eventos que publica (crate `contratos`)

- `RespuestaWhatsAppRecibida`: un cliente tocó un botón del recordatorio.

## Estructura

```text
notificaciones/src/
├── api.rs                       # BuzonSalida, Avisos, ConsultaMensajes, plantillas, resúmenes
├── modulo.rs                    # DependenciasNotificaciones, envíos, webhooks y limpieza
├── dominio/                     # plantillas (correo y WhatsApp), reintentos y webhooks de Meta
├── aplicacion/                  # EnviarCorreos, EnviarWhatsApp y ProcesarWebhooks
├── infraestructura/
│   ├── postgres/                # cola, webhooks, avisos y registro
│   ├── smtp/                    # lettre: STARTTLS, TLS o sin cifrado (solo desarrollo)
│   └── whatsapp_simulado.rs     # adaptador de desarrollo y pruebas
└── http/                        # avisos, registro, webhook y /_pruebas/mensajes
tests/cola_postgres.rs           # transacción revertida, reintentos, simulador con aviso y webhooks
```
