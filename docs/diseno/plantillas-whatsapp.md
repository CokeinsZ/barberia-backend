# Plantillas de WhatsApp registradas en Meta

**Proyecto**: Diamante Barbería · **Módulo**: `notificaciones` · **Versión**: 2.0 (23/11/2026)

Las plantillas están creadas en WhatsApp Manager en **español de Colombia** (`es_CO`). El
backend solo envía a Meta el **nombre**, el **idioma** y los **parámetros en orden**: Meta arma
el mensaje con el texto que tiene registrado. El texto de abajo es el mismo que vive en el código
(`modulos/notificaciones/src/dominio/whatsapp.rs`) para el simulador y las pruebas; una prueba
verifica que este documento lo copie al pie de la letra. **Si cambia una plantilla en Meta,
cambie también el código y este documento** (sobre todo el número y el orden de las variables:
si no coinciden, Meta rechaza el mensaje con el error 132000).

## Cómo se envían

- Con `DIAMANTE_WHATSAPP_TOKEN` y `DIAMANTE_WHATSAPP_ID_NUMERO` configurados, el worker envía por
  **WhatsApp Cloud API** (`POST https://graph.facebook.com/v26.0/{id_numero}/messages`). Idioma:
  `DIAMANTE_WHATSAPP_IDIOMA` (por defecto `es_CO`); versión de la API: `DIAMANTE_WHATSAPP_API`.
- Sin esas credenciales, fuera de producción se **simula** (el worker marca el mensaje como
  enviado y escribe el texto en el log; las pruebas lo leen con `GET /api/v1/_pruebas/mensajes`)
  y en producción los mensajes **esperan en la cola** hasta configurarlas.
- El webhook (`POST /api/v1/webhooks/whatsapp`, suscrito al campo `messages`) recibe los estados
  de entrega y las respuestas a los botones; necesita `DIAMANTE_WHATSAPP_SECRETO_APP` y
  `DIAMANTE_WHATSAPP_TOKEN_VERIFICACION`.
- `{{2}}` de las plantillas de citas es el **nombre comercial** configurado en el negocio: se lee
  al enviar, así que un cambio de nombre se ve en los mensajes siguientes.
- Los parámetros van sin saltos de línea ni espacios de más (Meta los rechaza). Si en la
  plantilla una variable va seguida de un punto, el valor se envía sin su punto final: «a las
  {{6}}.» con «9:00 a. m.» queda «a las 9:00 a. m.» y no «a. m..».

## Errores de Meta

| Caso | Errores | Qué hace el worker |
|---|---|---|
| Temporales | Límites de envío (4, 17, 32, 80007, 130429, 131048, 131056), fallas de Meta (1, 2, 131000, 131016, 133004, HTTP 5xx), sin conexión | Reintenta a 1, 2, 4 y 8 minutos; al quinto intento queda fallido |
| Credenciales | 190 (token vencido o inválido), 10 y 200 (permisos) | Igual que los temporales (se corrigen en Dokploy) y se registra un `ERROR` en el log |
| Permanentes | El número no tiene WhatsApp (131026), plantilla inexistente o con otros parámetros (132xxx), parámetro inválido, número de prueba fuera de la lista permitida (131030)… | Fallido de inmediato |

Un mensaje de una cita que queda fallido crea un **aviso** para la administradora con el motivo.

## Plantillas

### 1. `codigo_verificacion` · Autenticación

Código para verificar el celular al reservar (RF-RES-09). Formato fijo de Meta para autenticación:
recomendación de seguridad, vencimiento de **10 minutos** y botón **Copiar código** (el backend
envía el código en el cuerpo y en el botón).

```text
*{{1}}* es tu código de verificación. Por tu seguridad, no lo compartas. Este código caduca en 10 minutos.
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Código de 4 dígitos | 4821 |

### 2. `cita_confirmada` · Utilidad

Confirmación al reservar en línea o cuando el personal agenda una cita (RF-NOT-01).

```text
Hola {{1}}.
Tu cita en {{2}} para el servicio {{3}} quedó agendada con {{4}} el {{5}} a las {{6}}.

Puedes verla, reprogramarla o cancelarla aquí: {{7}}

¡Te esperamos!
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Nombre del cliente | Andrea Gómez |
| `{{2}}` | Nombre comercial | Diamante Barbería |
| `{{3}}` | Servicio | Corte clásico |
| `{{4}}` | Barbero | Daniel Castro |
| `{{5}}` | Fecha | lunes 23 de noviembre |
| `{{6}}` | Hora (sin el punto final) | 9:00 a. m |
| `{{7}}` | Enlace de gestión | https://diamante.co/cita/Zk3vQ8 |

### 3. `recordatorio_cita` · Utilidad, con botones

Recordatorio automático 24 h y 2 h antes, y el manual desde la agenda (RF-NOT-02, RF-NOT-03, RF-NOT-05).

```text
Hola {{1}}.
Te recordamos tu cita en {{2}} el {{3}} a las {{4}} con {{5}}.
¿Nos confirmas tu asistencia?
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Nombre del cliente | Andrea Gómez |
| `{{2}}` | Nombre comercial | Diamante Barbería |
| `{{3}}` | Fecha | lunes 23 de noviembre |
| `{{4}}` | Hora | 9:00 a. m. |
| `{{5}}` | Barbero | Daniel Castro |

**Botones de respuesta rápida** (en este orden):

| Texto del botón | Carga (`payload`) que envía el backend | Qué hace al tocarlo |
|---|---|---|
| Confirmo | `confirmar` | Una cita «Sin confirmar» pasa a «Confirmada» (RN-31). |
| Cancelar cita | `cancelar` | Cancela la cita (tardía si falta menos que el aviso mínimo) y envía `cita_cancelada`. |

### 4. `cita_reprogramada` · Utilidad

Aviso cuando el personal cambia la fecha, la hora o el barbero (RF-NOT-04). Trae un enlace nuevo: el anterior deja de servir.

```text
Hola {{1}}.
Tu cita en {{2}} cambió.
Ahora es el {{3}} a las {{4}}, con {{5}}.

Puedes gestionarla aquí: {{6}}

¡Te esperamos!
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Nombre del cliente | Andrea Gómez |
| `{{2}}` | Nombre comercial | Diamante Barbería |
| `{{3}}` | Fecha nueva | martes 24 de noviembre |
| `{{4}}` | Hora nueva | 3:30 p. m. |
| `{{5}}` | Barbero | Felipe Rojas |
| `{{6}}` | Enlace de gestión nuevo | https://diamante.co/cita/Pq81Tz |

### 5. `cita_cancelada` · Utilidad

Aviso de cancelación, por el personal (si marca «avisar al cliente») o como respuesta al botón «Cancelar cita» (RF-NOT-04).

```text
Hola {{1}}.
Tu cita en {{2}} de {{3}} de la fecha {{4}} fue cancelada.
Si quieres, puedes agendar otra en nuestra página.
¡Gracias!
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Nombre del cliente | Andrea Gómez |
| `{{2}}` | Nombre comercial | Diamante Barbería |
| `{{3}}` | Servicio | Corte clásico |
| `{{4}}` | Fecha con la hora | lunes 23 de noviembre a las 9:00 a. m. |

### 6. `aviso_reprogramacion` · Utilidad

Pide al cliente elegir otra hora cuando el barbero no podrá atenderlo (RF-BAR-14). Trae un enlace nuevo de gestión.

```text
Hola {{1}}, por un imprevisto no podremos atenderte el {{2}} a las {{3}} en tu cita de {{4}}. Elige otra hora aquí: {{5}} Disculpa las molestias.
```

| Variable | Contenido | Ejemplo |
|---|---|---|
| `{{1}}` | Nombre del cliente | Andrea Gómez |
| `{{2}}` | Fecha | lunes 23 de noviembre |
| `{{3}}` | Hora | 9:00 a. m. |
| `{{4}}` | Servicio | Corte clásico |
| `{{5}}` | Enlace de gestión nuevo | https://diamante.co/cita/Lm52Xa |

## Comportamiento del simulador

| Celular de destino | Resultado simulado |
|---|---|
| Termina en `0000` | Falla temporal: el mensaje se reintenta (1, 2, 4 y 8 minutos) y al quinto intento queda fallido con un aviso para la administradora (E2E-NOT-04). |
| Termina en `9999` | Falla permanente (número sin WhatsApp): queda fallido de inmediato, con aviso. |
| Cualquier otro | Enviado, con un id `wamid.simulado.…`. |

En modo simulado las variables de los mensajes se conservan (nunca salen del sistema) para que las
pruebas lean los códigos; con Meta, las de las plantillas secretas (código y enlaces) se borran al
enviarse.
