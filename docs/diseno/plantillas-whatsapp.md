# Plantillas de WhatsApp para registrar en Meta

**Proyecto**: Diamante Barbería · **Módulo**: `notificaciones` · **Versión**: 1.0 (21/11/2026)

Mientras las plantillas no estén aprobadas en Meta, el backend **simula** el envío fuera de
producción: el worker marca cada mensaje como enviado y escribe en el log el texto exacto de
esta tabla, y las pruebas leen el mensaje con `GET /api/v1/_pruebas/mensajes`. En producción, sin
credenciales de Meta, los mensajes quedan en la cola hasta que se configuren.

El texto de cada plantilla vive en el código (`modulos/notificaciones/src/dominio/whatsapp.rs`) y una
prueba verifica que este documento lo copie al pie de la letra: si cambia uno, cambia el otro.

## Cómo registrarlas

1. En **WhatsApp Manager → Plantillas de mensajes**, cree cada plantilla con el **nombre** exacto de
   la tabla (en minúsculas, con guiones bajos), el idioma **Español (`es`)** y la **categoría** indicada.
2. Copie el **cuerpo** tal cual. Las variables `{{1}}`, `{{2}}`… se llenan en ese orden con los
   valores de la columna *Variables*. Meta pide un ejemplo de cada variable: use los de la tabla.
3. Agregue los **botones** de respuesta rápida cuando la tabla los indique. La *carga* es lo que el
   backend recibe en el webhook cuando el cliente toca el botón.
4. Cuando Meta las apruebe, configure las credenciales (fase de integración real) y el webhook
   `POST /api/v1/webhooks/whatsapp` con el **token de verificación** (`DIAMANTE_WHATSAPP_TOKEN_VERIFICACION`)
   y el **secreto de la app** (`DIAMANTE_WHATSAPP_SECRETO_APP`), suscrito al campo `messages`.

Reglas de Meta que ya cumplen los textos: ninguna plantilla de utilidad empieza ni termina con una
variable, no hay variables seguidas, las variables aparecen en orden y el tono es transaccional (no
promocional). Además, la hora («9:00 a. m.») nunca va seguida de un punto, para no terminar en «a. m..».

## Plantillas

### 1. `codigo_verificacion` · Autenticación

Código para verificar el celular al reservar (RF-RES-09). Meta usa un formato fijo para las plantillas
de autenticación: active «Agregar recomendación de seguridad», el vencimiento de **10 minutos** y el
botón **Copiar código**.

- **Cuerpo**: *{{1}}* es tu código de verificación. Por tu seguridad, no lo compartas. Este código caduca en 10 minutos.
- **Variables**: `{{1}}` código de 4 dígitos (ejemplo: `4821`).
- **Texto exacto en el código**: `*{{1}}* es tu código de verificación. Por tu seguridad, no lo compartas. Este código caduca en 10 minutos.`

### 2. `cita_confirmada` · Utilidad

Confirmación al reservar en línea o cuando el personal agenda una cita (RF-NOT-01).

- **Cuerpo**: `Hola {{1}}, tu cita en Diamante Barbería quedó agendada para el {{2}} a las {{3}}: {{4}} con {{5}}. Puedes verla, reprogramarla o cancelarla aquí: {{6}} ¡Te esperamos!`
- **Variables**: `{{1}}` nombre (Andrea Gómez) · `{{2}}` fecha (lunes 23 de noviembre) · `{{3}}` hora (9:00 a. m.) · `{{4}}` servicio (Corte clásico) · `{{5}}` barbero (Daniel Castro) · `{{6}}` enlace de gestión (https://diamante.co/cita/Zk3v…).

### 3. `recordatorio_cita` · Utilidad, con botones

Recordatorio automático 24 h y 2 h antes, y el manual desde la agenda (RF-NOT-02, RF-NOT-03, RF-NOT-05).

- **Cuerpo**: `Hola {{1}}, te recordamos tu cita en Diamante Barbería el {{2}} a las {{3}}: {{4}} con {{5}}. ¿Nos confirmas tu asistencia?`
- **Variables**: `{{1}}` nombre · `{{2}}` fecha · `{{3}}` hora · `{{4}}` servicio · `{{5}}` barbero.
- **Botones de respuesta rápida**:

| Texto del botón | Carga (`payload`) | Qué hace el backend |
|---|---|---|
| Confirmo | `confirmar` | Una cita «Sin confirmar» pasa a «Confirmada» (RN-31). |
| Cancelar cita | `cancelar` | Cancela la cita (tardía si falta menos que el aviso mínimo) y envía `cita_cancelada`. |

### 4. `cita_reprogramada` · Utilidad

Aviso cuando el personal cambia la fecha, la hora o el barbero (RF-NOT-04). Trae un enlace nuevo: el anterior deja de servir.

- **Cuerpo**: `Hola {{1}}, tu cita en Diamante Barbería cambió: ahora es el {{2}} a las {{3}}, {{4}} con {{5}}. Puedes gestionarla aquí: {{6}} Si no puedes asistir, avísanos.`
- **Variables**: iguales a `cita_confirmada`.

### 5. `cita_cancelada` · Utilidad

Aviso de cancelación, por el personal (si marca «avisar al cliente») o como respuesta al botón «Cancelar cita» (RF-NOT-04).

- **Cuerpo**: `Hola {{1}}, tu cita de {{2}} del {{3}} a las {{4}} en Diamante Barbería fue cancelada. Si quieres, agenda otra en nuestra página. ¡Gracias!`
- **Variables**: `{{1}}` nombre · `{{2}}` servicio · `{{3}}` fecha · `{{4}}` hora.

### 6. `aviso_reprogramacion` · Utilidad

Pide al cliente elegir otra hora cuando el barbero no podrá atenderlo (RF-BAR-14). Trae un enlace nuevo de gestión.

- **Cuerpo**: `Hola {{1}}, por un imprevisto no podremos atenderte el {{2}} a las {{3}} en tu cita de {{4}}. Elige otra hora aquí: {{5}} Disculpa las molestias.`
- **Variables**: `{{1}}` nombre · `{{2}}` fecha · `{{3}}` hora · `{{4}}` servicio · `{{5}}` enlace de gestión.

## Comportamiento del simulador

| Celular de destino | Resultado simulado |
|---|---|
| Termina en `0000` | Falla temporal: el mensaje se reintenta (1, 2, 4 y 8 minutos) y al quinto intento queda fallido con un aviso para la administradora (E2E-NOT-04). |
| Termina en `9999` | Falla permanente (número sin WhatsApp): queda fallido de inmediato, con aviso. |
| Cualquier otro | Enviado, con un id `wamid.simulado.…`. |

En modo simulado las variables de los mensajes se conservan (nunca salen del sistema) para que las
pruebas lean los códigos; con Meta, las de las plantillas secretas (código y enlaces) se borran al enviarse.
