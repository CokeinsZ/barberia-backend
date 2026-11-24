# Pruebas E2E contra preproducción

Prueban el backend **desplegado** (por HTTPS, con su configuración de producción) de punta a
punta: todas las rutas del panel y del sitio público, los flujos completos (reserva en línea,
gestión por enlace, agenda, venta, caja, reportes y PDF), la seguridad (cabeceras, CSRF, matriz
de autorización, límites de uso) y la concurrencia con peticiones simultáneas por la red.

Para probar en local con todas las herramientas de prueba (reloj, WhatsApp simulado, Mailpit)
use `make coleccion`; esta batería es la que sirve contra un servidor real.

## Cómo se corre

```bash
cd pruebas/preproduccion
DIAMANTE_URL=https://back-diamante.mania-agent.online \
DIAMANTE_SSH=debian@cokeinsz.com \
node correr.mjs
```

- Desde la raíz: `make preproduccion URL=https://… SSH=debian@…`.
- Node 18 o superior, sin dependencias.
- `ESCENARIOS=07,12` corre solo esos (más 00, 02, 03 y 99, de los que dependen).
- Resultado en pantalla y en `resultados/<corrida>.json` (fuera de git).

**La primera vez** (o tras reiniciar la base), para activar la cuenta de la administradora creada con
`diamante crear-administrador`, agregue `DIAMANTE_CORREO_ADMIN=…` y
`DIAMANTE_TOKEN_ACTIVACION=…` (el token que va después de `#token=` en el enlace). La batería
crea una contraseña aleatoria y la guarda en `.credenciales.json` (permisos 600, fuera de git);
de ahí en adelante entra con ella. Para entrar al panel con esa cuenta, use esa contraseña o
«Olvidé mi contraseña».

## SSH (opcional, recomendado)

En producción no existen las rutas de apoyo de pruebas, así que dos cosas se leen del servidor
por SSH, solo lectura:

- **Códigos de verificación de WhatsApp**: sin credenciales de Meta, los mensajes quedan en la
  cola con sus variables; la batería lee el código con una consulta `SELECT`.
- **Cuenta del barbero**: las invitaciones van a direcciones `@example.com` (no le llegan a
  nadie), así que el enlace de activación se pide con `diamante enlace-acceso <correo>` dentro
  del contenedor de la API (requiere la versión que tiene ese comando).

La aplicación se detecta sola (el contenedor con la imagen `diamante-backend`); con varias,
fíjela con `DIAMANTE_APP=<nombre en Dokploy>`. Sin `DIAMANTE_SSH`, esos pasos se omiten.

## Escenarios

| | Qué prueba |
|---|---|
| 00 | Salud, cabeceras de seguridad (HSTS, CSP…), HTTP → HTTPS, 404 en *Problem Details*, CSRF por `Origin`, credenciales inválidas, inicio de sesión y cookie segura |
| 01 | Administradores (invitar, duplicado, validación, desactivar), recuperación, enlaces inválidos, cambio de contraseña inválido |
| 02 | Negocio (sin cambiarlo), días especiales, festivos, política de datos, categorías y servicios (combo, duplicados, edición, estado) |
| 03 | Tres barberos (horario 00:00–23:30 para que siempre haya turnos de hoy): edición, visibilidad, orden, horario, servicios, datos privados cifrados, foto, expediente con descarga firmada (y firma alterada), ausencia con soporte, bloqueos, conflictos |
| 04 | Cuenta del barbero: activación, «Mi perfil», su horario y bloqueos (y no los de otro), cambio de contraseña |
| 05 | Clientes: registro, duplicado, búsqueda, ficha, edición, solicitud del titular, anonimización |
| 06 | Sitio público: landing, servicios, barberos, calendario, horas, verificación por WhatsApp, reserva, comprobante de un solo uso, gestión por enlace (ver, recordatorios, reprogramar, cancelar), «Cualquier barbero» |
| 07 | Agenda: crear, conflicto, listar, tablero, confirmar, recordatorio, reprogramar, reasignar, aviso de reprogramación, bloqueo con citas (aviso), cancelar, iniciar e inasistencia de hoy, configuración |
| 08 | Ventas: cobro desde la cita (descuento, propina, dos métodos), validaciones, cliente nuevo/ocasional/registrado, listado, anulación auditada, caja, «Lo que llevas hoy», panel del barbero, cierre, métodos de pago |
| 09 | Reportes: resumen, hoy = caja, dashboard, tendencias, alertas, clientes, liquidación, 4 PDF con Gotenberg y su auditoría, validaciones, «Mi reporte» |
| 10 | Mensajes (sin variables), avisos internos, webhook sin configurar |
| 11 | Matriz de autorización: 48 rutas sin sesión, rutas de administradora con el barbero, cookie inventada |
| 12 | Concurrencia: 8 reservas a la misma hora, 8 con «Cualquier barbero», reprogramaciones simultáneas, 10 ventas a la vez (tiquetes), cobro y anulación dobles, celular repetido, 10 ventas al mismo cliente, transiciones cruzadas, reserva en línea simultánea, ráfagas de 40 y 60 peticiones con latencias |
| 12b | Entrega 2: umbrales de los segmentos (cambian el listado y los reportes al instante; se dejan como estaban), indicadores de la base contra el filtro por segmento, «Mis citas» del barbero, anular y volver a cobrar una cita, aviso al retirar a un barbero con citas (uno propio del paso), vigilancia (`/api/salud/operacion` y `/sistema/estado` sin alertas) y entradas hostiles por la red (carácter nulo, fechas de otro milenio, montos absurdos): nunca un `5xx` |
| 13 | Límites de uso: 429 con `Retry-After` y `X-Forwarded-For` falso (al final: agota el límite público un minuto) |
| 99 | Limpieza: cancela las citas pendientes de la corrida, retira a sus barberos, desactiva sus servicios y cierra las sesiones |

## Con WhatsApp de verdad, solo con lista

La batería crea clientes con **celulares inventados**: con WhatsApp conectado a Meta, los
mensajes (códigos, confirmaciones, recordatorios) les llegarían a personas reales. Después de
iniciar sesión revisa `GET /api/v1/sistema/estado` y, si los WhatsApp salen por Meta, **se detiene
antes de crear nada**, salvo que el servidor tenga `DIAMANTE_WHATSAPP_SOLO_A` (runbook, sección
3): entonces solo los celulares de esa lista reciben WhatsApp de verdad y a los demás se les
simula. Para probar con Meta, use `whatsapp-real.mjs` con su propio celular (que debe estar en la
lista).

## WhatsApp de verdad a su celular (`whatsapp-real.mjs`)

Para probar Meta: manda **cada plantilla a un solo celular, el suyo**, y muestra cómo queda cada
mensaje (`enviado`, `entregado`, `leido` o `fallido` con el motivo de Meta).

```bash
make whatsapp-real URL=https://back-diamante.mania-agent.online CELULAR=573003579384
```

- Exige que el servidor envíe por Meta (`proveedor_whatsapp: meta` en `/sistema/estado`).
- Crea un barbero y un servicio propios (ocultos en la landing) y **siempre** los deja retirado y
  desactivado, aunque la prueba falle a mitad de camino. Al empezar también recoge los que haya
  dejado una corrida anterior que se cortó (con Ctrl+C, por ejemplo); para hacer solo eso, sin
  enviar nada: `DIAMANTE_URL=https://… node whatsapp-real.mjs --solo-limpiar`. El cliente es el de
  su celular (si ya existía, se usa ese; `NOMBRE=…` para el nombre de uno nuevo).
- Orden: confirmación, recordatorio (con «Confirmo» y «Cancelar cita»), cambio de hora, aviso para
  elegir otra hora, cancelación y código de verificación.
- En una terminal (o con `--interactivo`) le pide tocar **«Confirmo»** y espera a que el webhook de
  Meta confirme la cita, y le pide **el código** para hacer una reserva en línea con él. Con
  `--sin-preguntas` solo envía.
- La reserva en línea necesita una **política de tratamiento de datos vigente** (la misma que exige
  la reserva real). En una base nueva no la hay: la prueba lo avisa antes de pedir el código y solo
  envía el código. Publíquela en el panel (o `POST /api/v1/negocio/politicas`) y vuelva a correrla.
- Usa las credenciales de la batería. Si la base es nueva, cree la administradora y pase el token:
  `DIAMANTE_CORREO_ADMIN=… DIAMANTE_TOKEN_ACTIVACION=… make whatsapp-real …` (el token es lo que va
  después de `#token=` en el enlace de `diamante crear-administrador`).

## Límites de producción (por IP)

- **Inicios de sesión: 10 cada 15 minutos.** Cada corrida usa 2 (4 con el barbero): no la corra
  más de dos veces cada 15 minutos. Si se agota, la batería se detiene enseguida y dice cuánto
  esperar. El límite también afecta al panel desde la misma IP.
- **Códigos de verificación: 10 por hora** (cada corrida usa 6, contando el celular inválido):
  para probar completa la reserva en línea, deje una hora entre corridas. Si se agotan, esos
  pasos se omiten y dicen cuántos segundos faltan.
- **Recuperación de contraseña: 10 por hora** (cada corrida usa 2).

Los contadores de intentos viven en la memoria de la API: **un redespliegue los reinicia**.

## Qué deja en la base

Todo lo que crea lleva el sufijo `E2E <corrida>`: barberos (retirados al final; el del paso de
retiro, desde el día siguiente), servicios (desactivados), clientes, citas (canceladas) y ventas
(son historia: no se borran; una de cada corrida queda anulada y vuelta a cobrar). Los correos
de invitación van a `@example.com`. Los WhatsApp quedan **en cola** mientras no haya credenciales
de Meta: **antes de pasar a producción con Meta configurado, reinicie la base de preproducción**
(borrar el volumen `…_db_datos`), o esos mensajes a números de prueba saldrían.
