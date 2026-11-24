# Diseño de la API REST

**Proyecto**: Diamante Barbería · **Backend**: Rust + Axum · **Versión del documento**: 1.1 (23/11/2026)

La API la consumen el frontend (Next.js) y Meta (webhooks de WhatsApp). Este documento describe la API **implementada**: lo que aún no existe está marcado *Pendiente*. Una prueba del backend (`crates/app/src/pruebas_documentacion.rs`) falla si una ruta del código no aparece aquí, si aquí aparece una ruta que no existe sin estar marcada *Pendiente*, o si la tabla de errores (1.3) no tiene exactamente los códigos que responde el código.

Para el frontend, la capa `frontend/src/api/` tiene una función por endpoint y los tipos de cada respuesta (`tipos.ts`), y `frontend/src/api/api.md` resume qué llamar y qué esperar.

> **Historia.** La versión 1.0 (18/11) se escribió antes de programar, como el contrato a construir. Al implementar cambiaron algunas cosas y el documento no se actualizó: la respuesta de `POST /ventas` (ahora la venta completa), `GET /yo` (sin la próxima cita: barberos no puede depender de agenda), los códigos de error (cada módulo tiene los suyos) y rutas opcionales que no se hicieron. La 1.1 lo alinea con el código y agrega la prueba para que no vuelva a pasar.

---

## 1. Convenciones

| Tema | Convención |
|---|---|
| Base | `https://<dominio>/api/v1`. El proxy (Traefik de Dokploy) enruta `/api/*` y `/media/*` al backend; el sitio y la API comparten dominio. |
| Formato | JSON en `snake_case` y UTF-8. Archivos con `multipart/form-data`. PDF, Excel e ICS como descargas. |
| Dinero | Enteros en pesos (`35000`). Nunca decimales. |
| Fechas y horas | ISO 8601: fechas `2026-11-19`; instantes con zona `2026-11-19T15:30:00-05:00`. El servidor interpreta las fechas sin hora en America/Bogota. |
| Identificadores | Números enteros. Los enlaces públicos usan tokens opacos, nunca IDs. |
| Paginación | `?pagina=1&por_pagina=20` (máximo 100). Respuesta: `{ "datos": [...], "pagina": 1, "por_pagina": 20, "total": 1284 }`. |
| Filtros y búsqueda | Parámetros de consulta en español (`?q=`, `?estado=`, `?barbero_id=`, `?desde=`, `?hasta=`). |
| Acciones | Los cambios de estado que no son una edición son subrecursos con `POST` (`/citas/{id}/cancelar`). Así cada uno tiene su permiso, su validación y su auditoría. |
| Versionado | En la ruta (`/v1`). Un cambio incompatible crea `/v2`. |
| Idempotencia | Por diseño: el comprobante de verificación es de un solo uso (una reserva por código), una cita se cobra una sola vez (`UNIQUE (cita_id)`) y los botones de envío se bloquean mientras esperan respuesta. |

### 1.1 Autenticación y sesión

- **Personal** (administrador y barbero): `POST /auth/sesion` crea una sesión en el servidor y devuelve la cookie `__Host-sesion` (`HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/`). La sesión dura 12 horas de inactividad, o 30 días con «Recordarme».
- **Cliente**: no tiene contraseña. Se identifica con un código de WhatsApp que entrega un **comprobante** de un solo uso para crear su cita, y gestiona su cita con el **token** de su enlace.
- **CSRF**: además de `SameSite=Strict`, toda petición que modifica datos exige que el encabezado `Origin` sea el del sitio.
- **Webhooks de Meta**: se valida la firma `X-Hub-Signature-256` con el secreto de la app.

### 1.2 Autorización

Como en el proyecto de referencia (*guards* y *abilities* por rol), pero en la capa de aplicación de cada módulo: cada caso de uso declara su **política**, que el servidor evalúa siempre.

| Perfil | Regla general |
|---|---|
| Público | Solo las rutas de `/publico`, `/auth` y `/webhooks`. |
| Barbero | Solo lo propio: las políticas fijan `barbero_id` al de la sesión, aunque la petición traiga otro. Pedir un recurso ajeno responde `404`, para no revelar su existencia. |
| Administrador | Todo. |

En las tablas de la sección 2, «Barbero (propio)» significa que el barbero puede usar ese endpoint solo sobre sus datos.

### 1.3 Errores

Formato *Problem Details* (RFC 9457), con `Content-Type: application/problem+json`:

```json
{
  "type": "https://diamante.co/errores/conflicto-horario",
  "title": "La hora ya no está disponible",
  "status": 409,
  "codigo": "conflicto_horario",
  "detail": "Otra persona acaba de reservar el jueves 19 de noviembre a las 3:30 p. m. con Santiago.",
  "errores": [],
  "alternativas": ["2026-11-19T16:45:00-05:00", "2026-11-19T17:30:00-05:00"]
}
```

Todos los códigos que responde el backend (la prueba de documentación verifica que la tabla tenga
exactamente estos):

| Estado | `codigo` | Cuándo |
|---|---|---|
| 400 | `solicitud_invalida` | JSON mal formado, un campo que falta o sobra, o parámetros imposibles de interpretar. |
| 401 | `no_autenticado` | Falta la sesión, o está vencida o revocada. |
| 401 | `credenciales_invalidas` | Correo o contraseña incorrectos (genérico: también con la cuenta bloqueada). |
| 401 / 403 | `firma_invalida` | `401`: la firma de un webhook de WhatsApp no coincide. `403`: la firma de una descarga (`/archivos/{id}`) no es válida. |
| 403 | `prohibido` | La política del caso de uso no lo permite. |
| 403 | `origen_invalido` | La verificación CSRF falló (`Origin` ausente o ajeno en una petición que modifica datos). |
| 403 | `acceso_suspendido` | La cuenta no tiene acceso al panel (desactivada, barbero suspendido o retirado). |
| 403 | `reserva_bloqueada` | El cliente superó el umbral de inasistencias y la política es «bloquear la reserva en línea». |
| 404 | `no_encontrado` | La ruta o el recurso no existe, o no es del barbero. |
| 408 | (sin cuerpo) | La petición superó el tiempo máximo (30 s; 120 s los reportes). |
| 409 | `conflicto_horario` | La hora ya no está libre; trae `alternativas` (instantes cercanos libres). |
| 409 | `estado_invalido` | La acción no aplica al estado actual: cancelar una cita atendida, anular una venta anulada, anonimizar un cliente con citas por atender, responder dos veces una solicitud, dejar sin métodos de pago, recordatorio desactivado o repetido… |
| 409 | `cita_ya_cobrada` | La cita ya tiene una venta vigente. |
| 409 | `cierre_existente` | La caja de ese día ya se cerró. |
| 409 | `correo_en_uso` | Otro usuario (administrador o barbero) ya tiene ese correo. |
| 409 | `celular_en_uso` | Otro cliente ya tiene ese celular. |
| 409 | `documento_en_uso` | Otro barbero ya tiene ese número de documento. |
| 409 | `nombre_en_uso` | Ya hay un servicio con ese nombre. |
| 409 | `no_es_combo` | Se cambiaron los componentes de un servicio simple. |
| 409 | `fecha_ocupada` | Ya hay un día especial en esa fecha. |
| 409 | `barbero_retirado` | El barbero está retirado: ya no se le cambian datos, visibilidad, horario, servicios, ausencias ni bloqueos, ni se retira de nuevo. |
| 409 | `ausencia_solapada` | La ausencia se cruza con otra vigente. |
| 409 | `bloqueo_solapado` | El bloqueo se cruza con otro. |
| 409 | `planilla_repetida` | Ya hay una planilla PILA de ese mes. |
| 409 | `ultimo_administrador` | Desactivarlo dejaría el sistema sin administradores activos. |
| 409 | `no_puede_desactivarse` | Un administrador intentó desactivarse a sí mismo. |
| 410 | `enlace_vencido` | Token de activación, de recuperación, de gestión de cita o de descarga vencido o ya usado. |
| 410 | `codigo_vencido` | El código de WhatsApp venció (10 minutos) o se invalidó (5 intentos). |
| 410 | `comprobante_vencido` | El comprobante de la verificación ya se usó o venció. |
| 413 | `archivo_demasiado_grande` | El formulario supera el límite de la ruta (6 MB la foto, 11 MB documentos y soportes). Un archivo que solo pasa del máximo permitido (5 MB y 10 MB) responde `422 validacion` en su campo. |
| 413 | `cuerpo_demasiado_grande` | El cuerpo JSON supera el límite. |
| 422 | `validacion` | Reglas de negocio o de formato; `errores` trae `[{ "campo": "celular", "mensaje": "…" }]`. |
| 422 | `codigo_incorrecto` | El código de WhatsApp no coincide (quedan intentos). |
| 429 | `demasiados_intentos` | Límite de intentos (1.4); incluye el encabezado `Retry-After`. |
| 500 | `error_interno` | Error inesperado (también un pánico, que no corta la conexión); el encabezado `x-request-id` sirve para rastrearlo en los logs. |
| 503 | `pdf_no_disponible` | El generador de PDF (Gotenberg) no responde. |

Además de las reglas de cada módulo, **toda entrada pasa por una revisión común** (cuerpo JSON,
parámetros de la URL y campos de un formulario): un texto con el carácter nulo (`\u0000`) o una
fecha (`AAAA-MM-DD`, `AAAA-MM` o un instante ISO 8601) fuera de los años 1900 a 2999 responde
`422 validacion` con el campo (`cliente_nuevo.nombre`, `pagos[1].monto`). Un valor en pesos por
encima de un billón (10¹²) se rechaza con `422` en su campo (o `400` si ni siquiera cabe en un
entero), nunca se convierte en otro valor. Ninguna entrada, por rara que sea, produce un `5xx`:
lo comprueba una prueba que recorre todas las rutas con valores hostiles (`pruebas_robustez.rs`).

### 1.4 Límites de uso (por IP, salvo indicación)

| Grupo | Límite |
|---|---|
| `POST /publico/verificaciones` | 3 por celular cada 15 min y 10 por IP por hora |
| `POST /publico/verificaciones/{id}/confirmar` | 5 intentos por código (luego se invalida) |
| `POST /auth/sesion` | 10 por IP cada 15 min; la cuenta se bloquea 15 min tras 5 fallos |
| `POST /auth/recuperacion` | 3 por correo y 10 por IP por hora (siempre responde 202) |
| Resto de `/publico` | 120 por minuto |
| Panel | 600 por minuto por sesión (y 1.200 por minuto por IP, para que inventar cookies no sirva) |
| Otras rutas sin sesión | 120 por minuto |

Al superarlos: `429 demasiados_intentos` con `Retry-After`. No se limitan la salud, los webhooks firmados ni las fotos públicas. Detrás del proxy (Traefik), la IP es la que él agrega a `X-Forwarded-For`; un valor falso del cliente no la cambia. Solo las pruebas de carga los apagan (`DIAMANTE_LIMITES_DE_USO=false`, rechazado en producción).

### 1.5 Cabeceras y tiempos

Toda respuesta de la API lleva `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'`, `X-Robots-Tag: noindex`, `x-request-id`, `Cache-Control: no-store` (salvo la landing pública, 60 s) y, si `DIAMANTE_URL_PUBLICA` es `https`, `Strict-Transport-Security: max-age=31536000`. Tiempo máximo por petición: 30 s; los reportes, 120 s (`408` al superarlo).

---

## 2. Endpoints por módulo

### 2.1 Público — sitio y agendamiento (módulos `negocio`, `catalogo`, `barberos`, `agenda`, `verificacion`)

| Método | Ruta | Descripción | Requisitos |
|---|---|---|---|
| GET | `/publico/landing` | Datos de la landing en una sola llamada: negocio, horario de atención, servicios y combos visibles, barberos visibles con su estado («de vacaciones hasta…») y políticas vigentes. Respuesta con `Cache-Control: public, max-age=60`. | RF-PUB-01…04, RF-CFG-03 |
| GET | `/publico/servicios` | Servicios y combos agendables en línea que al menos un barbero ofrece. | RF-RES-02 |
| GET | `/publico/barberos?servicio_id=` | Opciones de barbero para el servicio, con su estado, motivo si no se puede elegir y próxima hora libre. | RF-RES-03, RF-RES-04 |
| GET | `/publico/disponibilidad/calendario?servicio_id=&barbero_id=&mes=2026-11` | Para cada día del mes: `disponible`, `ultimos_cupos` (≤ 3 horas), `sin_cupos` o `fuera_de_ventana` (pasado o más allá de la ventana). `barbero_id` vacío significa «Cualquier barbero». | RF-RES-05, RF-SIS-01 |
| GET | `/publico/disponibilidad/horas?servicio_id=&barbero_id=&fecha=` | Horas de inicio libres agrupadas en mañana, tarde y noche. Con «Cualquier barbero», cada hora trae el barbero que se asignaría. | RF-RES-05, RF-SIS-02 |
| POST | `/publico/verificaciones` | `{ "celular": "3105559917" }` → envía el código por WhatsApp. Responde `202` con `{ id, expira_en, reenviar_desde }`. | RF-RES-09 |
| POST | `/publico/verificaciones/{id}/confirmar` | `{ "codigo": "4821" }` → `{ comprobante, cliente }`. `cliente` trae nombre, correo y fecha de nacimiento si el celular ya existe: el autocompletado ocurre después de verificar. | RF-RES-08, RF-RES-09 |
| POST | `/publico/citas` | Crea la cita (ver ejemplo en 3.1). Responde `201`, o `409 conflicto_horario` con alternativas, `410 comprobante_vencido` (comprobante usado o vencido) o `403 reserva_bloqueada`. Exige una política `tratamiento_datos` vigente (`politica_version_id`): sin ella nadie puede reservar en línea. | RF-RES-07, RF-RES-10, RF-RES-11, RF-NOT-01 |
| GET | `/publico/citas/gestion/{token}` | Ver la cita desde el enlace de WhatsApp: estado, servicio, barbero, fecha y hora, dirección, total y qué acciones permite. | RF-RES-12, RF-RES-14 |
| POST | `/publico/citas/gestion/{token}/cancelar` | Cancela hasta la hora de inicio; dentro del aviso mínimo queda marcada como tardía. | RF-RES-14, RN-09 |
| POST | `/publico/citas/gestion/{token}/reprogramar` | `{ "inicio": "…", "barbero_id": null }`. Mismas reglas de disponibilidad. | RF-RES-15 |
| PUT | `/publico/citas/gestion/{token}/recordatorios` | `{ "activos": false }`. | RF-RES-18 |
| — | ~~`/publico/citas/gestion/{token}/calendario.ics`~~ | **Descartado (DA-18)**: solo enlace de Google Calendar (`calendario_google`). | RF-RES-17 (Could) |

### 2.2 Webhooks (módulo `notificaciones`)

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/webhooks/whatsapp` | Verificación de Meta (`hub.mode=subscribe`, `hub.verify_token`, `hub.challenge`): devuelve el reto solo con el token configurado (`403` si no). |
| POST | `/webhooks/whatsapp` | Estados de entrega y respuestas de botones. Verifica `X-Hub-Signature-256` con el cuerpo exacto (`401 firma_invalida`, sin guardar nada); guarda el evento y responde `200` de inmediato; el worker lo procesa (estados que solo avanzan, aviso si falla un mensaje de cita, «Confirmo» → cita confirmada, «Cancelar cita» → cancelada). Sin secretos configurados, `404`. |

### 2.3 Autenticación y administradores (módulo `identidad`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| POST | `/auth/sesion` | Público | `{ correo, contrasena, recordarme }` → cookie de sesión y `{ usuario: { id, rol, nombre, correo }, panel: "/admin" }`. `401 credenciales_invalidas` (genérico, también con la cuenta bloqueada) o `403 acceso_suspendido` (cuenta desactivada o barbero suspendido o retirado). | RF-AUT-01, RF-AUT-04 |
| GET | `/auth/sesion` | Personal | Usuario de la sesión actual. | RF-AUT-01 |
| DELETE | `/auth/sesion` | Personal | Cierra la sesión. | RF-AUT-07 |
| POST | `/auth/activacion` | Público | `{ token, contrasena }`: activa la cuenta desde la invitación. El enlace del correo es `/acceso/activar#token=…` (el token va en el fragmento para que no llegue a los logs). | RF-AUT-02 |
| POST | `/auth/recuperacion` | Público | `{ correo }` → `202` siempre, exista o no la cuenta. | RF-AUT-03 |
| POST | `/auth/recuperacion/confirmar` | Público | `{ token, contrasena }`. | RF-AUT-03 |
| PUT | `/auth/contrasena` | Personal | `{ actual, nueva }`; cierra las demás sesiones. | Propuesto |
| GET | `/administradores` | Admin | Lista de administradores. | RF-CFG-06 |
| POST | `/administradores` | Admin | `{ correo, nombre }` → `201 { id }` e invitación por correo; `409 correo_en_uso`. | RF-CFG-06 |
| PATCH | `/administradores/{id}` | Admin | `{ activo }`. No permite desactivar al último administrador activo (`409 ultimo_administrador`) ni a uno mismo (`409 no_puede_desactivarse`). Desactivar cierra sus sesiones. | RF-CFG-06 |

### 2.4 Negocio (módulo `negocio`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET / PUT | `/negocio` | Admin | Datos de la barbería y comisión por defecto. | RF-CFG-01 |
| GET | `/negocio/horario-atencion` | Personal | Horario calculado por día de la semana. | RF-CFG-03 |
| GET | `/negocio/dias-especiales?desde=&hasta=` | Personal | Festivos y cierres. | RF-CFG-04 |
| POST | `/negocio/dias-especiales` | Admin | `{ fecha, nombre, tipo, hora_cierre }`. | RF-CFG-04 |
| PUT / DELETE | `/negocio/dias-especiales/{id}` | Admin | Editar o quitar. | RF-CFG-04 |
| POST | `/negocio/dias-especiales/festivos/{anio}` | Admin | Precarga los festivos de Colombia del año (idempotente). | RF-CFG-05 |
| GET / POST | `/negocio/politicas` | Admin | Versiones de las políticas legales; publicar una versión nueva. | RNF-LEG-01 |

### 2.5 Catálogo de servicios (módulo `catalogo`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/categorias` | Personal | Categorías. | RF-SER-01 |
| GET | `/servicios?incluir_inactivos=true` | Personal | Servicios y combos con sus componentes y el número de barberos que los ofrecen. | RF-SER-01 |
| GET | `/servicios/{id}` | Personal | Detalle. | RF-SER-01 |
| POST | `/servicios` | Admin | Crea un servicio simple o un combo (`tipo`, `componentes` si es combo). | RF-SER-01, RF-SER-04, RF-SER-05, RF-SER-08 |
| PUT | `/servicios/{id}` | Admin | Edita. Un cambio de precio solo afecta citas y ventas nuevas. | RF-SER-01, RF-SER-03 |
| PATCH | `/servicios/{id}/estado` | Admin | `{ activo }`. | RF-SER-02 |
| PUT | `/servicios/{id}/componentes` | Admin | `{ servicio_ids: [...] }`, mínimo dos servicios simples. | RF-SER-01 |

### 2.6 Barberos (módulo `barberos`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/barberos?q=&estado=` | Admin | Listado con estado calculado, servicios e indicadores del equipo en `meta`. | RF-BAR-01, RF-BAR-02 |
| GET | `/barberos/exportacion.xlsx` | Admin | *Pendiente (Could).* Exporta el listado (auditado). | RF-BAR-03 (Could) |
| POST | `/barberos` | Admin | Registra: datos esenciales, servicios, horario y visibilidad. Crea la cuenta y, si tiene acceso, envía la invitación. | RF-BAR-04, RF-BAR-07…09 |
| GET / PUT | `/barberos/{id}` | Admin | Ficha y edición de datos. | RF-BAR-04 |
| GET / PUT | `/barberos/{id}/datos-privados` | Admin | Dirección, RH, emergencia, seguridad social y cuenta de pago (campos cifrados). | RF-BAR-05 |
| PUT | `/barberos/{id}/visibilidad` | Admin | `{ mostrar_en_landing, recibe_citas_en_linea, acceso_panel }`. | RF-BAR-09, RF-AUT-08 |
| PUT | `/barberos/orden` | Admin | `{ barbero_ids: [...] }`. | RF-BAR-19 |
| GET / PUT | `/barberos/{id}/horario` | Admin; Barbero (propio) | Franjas por día; reemplaza el horario completo. Publica `HorarioCambiado` (el administrador recibe un aviso si hay citas afectadas). | RF-BAR-07, RF-PBA-09 |
| GET / PUT | `/barberos/{id}/servicios` | Admin; Barbero (propio) | `{ servicio_ids: [...] }`. | RF-BAR-08, RF-PBA-09 |
| POST | `/barberos/{id}/foto` | Admin | Multipart (campo `foto`); imagen vertical JPG, PNG o WebP de hasta 5 MB. | RF-BAR-04 |
| GET / POST | `/barberos/{id}/documentos` | Admin | Lista y carga (multipart, con `tipo` y `periodo` para la PILA). | RF-BAR-06, RF-BAR-18 |
| GET | `/barberos/{id}/documentos/{doc_id}/descarga` | Admin | Redirige a una URL firmada de 5 minutos. | RNF-SEG-09 |
| DELETE | `/barberos/{id}/documentos/{doc_id}` | Admin | Quita el documento. | RF-BAR-06 |
| GET / POST | `/barberos/{id}/ausencias` | Admin | Vacaciones, incapacidad, licencia o suspensión con rango, observación y soporte. Publica `AusenciaRegistrada`. | RF-BAR-11, RF-BAR-12 |
| POST | `/barberos/{id}/ausencias/{aid}/anular` | Admin | Anula una ausencia. | RF-BAR-11 |
| POST | `/barberos/{id}/ausencias/{aid}/soporte` | Admin | Multipart (campo `soporte`): soporte de una incapacidad o licencia (privado). | RF-BAR-12 |
| POST | `/barberos/{id}/retiro` | Admin | `{ fecha, motivo }`; desactiva la cuenta y cierra sus sesiones. | RF-BAR-11, RF-BAR-15 |
| GET / POST | `/barberos/{id}/bloqueos` | Admin; Barbero (propio) | Bloqueos puntuales. | RF-PBA-10, RF-AGE-13 |
| DELETE | `/barberos/{id}/bloqueos/{bid}` | Admin; Barbero (propio) | Quita un bloqueo. | RF-PBA-10 |
| POST | `/barberos/{id}/solicitudes-correccion` | Barbero (propio) | *Pendiente (Could).* `{ campo, valor_propuesto }`. | RF-PBA-08 (Could) |
| GET | `/barberos/solicitudes-correccion?estado=pendiente` | Admin | *Pendiente (Could).* Solicitudes por atender. | RF-PBA-08 |
| PATCH | `/barberos/solicitudes-correccion/{id}` | Admin | *Pendiente (Could).* `{ estado }`. | RF-PBA-08 |
| GET | `/yo` | Barbero | Su ficha en solo lectura (la misma de `GET /barberos/{id}`), con su estado de hoy. Su próxima cita y su día salen de `GET /agenda/resumen-barbero` o `GET /agenda/citas`: `barberos` no consulta la agenda (es `agenda` la que depende de `barberos`; al revés sería un ciclo). | RF-PBA-07, RF-PBA-12 |

### 2.7 Clientes (módulo `clientes`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/clientes?q=&segmento=&pagina=` | Admin | Listado paginado con métricas, segmento y barbero habitual. `meta`: indicadores de toda la base (sin importar los filtros): `registrados`, `por_segmento` (`sin_visitas`, `nuevo`, `recurrente`, `frecuente`, `inactivo`) y `dias_inactividad`. «Nuevos del mes» y «vuelven en menos de 60 días» salen de las ventas: `GET /reportes/clientes`. | RF-CLI-02, RF-CLI-04 |
| GET | `/clientes/buscar?q=` | Admin; Barbero | Búsqueda rápida por nombre o celular para la agenda y la venta (máximo 8 resultados, solo nombre, celular y segmento). | RF-VEN-03 |
| POST | `/clientes` | Admin; Barbero | `{ nombre, celular, correo }`, desde el panel. | RF-CLI-01 |
| GET | `/clientes/{id}` | Admin | Ficha: datos, métricas, autorizaciones y preferencias. Las últimas visitas salen de `GET /ventas?cliente_id=`. | RF-CLI-03 |
| PUT | `/clientes/{id}` | Admin | Edita datos y preferencias. | RF-CLI-07 |
| POST | `/clientes/{id}/anonimizacion` | Admin | `{ motivo }` → `204`. Supresión por solicitud del titular: borra los datos personales, revoca la autorización y conserva las cifras; `409 estado_invalido` si tiene citas por atender. | RF-CLI-07, RNF-LEG-03 |
| GET | `/clientes/exportacion.xlsx` | Admin | *Pendiente (Could).* Exportación (auditada). | RF-CLI-06 (Could) |
| GET / POST | `/clientes/solicitudes-titular` | Admin | Registrar (`{ cliente_id, tipo: consulta \| actualizacion \| supresion, descripcion }`) y consultar (`?abiertas=true`) solicitudes de habeas data, con su plazo legal (`responder_antes_de`: 10 o 15 días hábiles). | RNF-LEG-03 |
| PATCH | `/clientes/solicitudes-titular/{id}` | Admin | `{ respuesta }`, una sola vez. | RNF-LEG-03 |
| GET / PUT | `/clientes/configuracion-segmentos` | Admin | Umbrales de los segmentos: `{ max_visitas_nuevo (1–99), max_visitas_recurrente (mayor, hasta 100), dias_inactividad (15–365) }`; auditados. Se aplican al consultar: listado, ficha, búsqueda y reportes («por reactivar»). | RF-CFG-08, RF-SIS-06 |

### 2.8 Agenda (módulo `agenda`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/agenda/tablero?fecha=` | Admin | Vista de día: una columna por barbero con sus citas y los bloques no disponibles (fuera de turno, descanso, festivo, vacaciones). | RF-AGE-01, RF-AGE-03 |
| GET | `/agenda/citas?desde=&hasta=&barbero_id=&estado=` | Admin; Barbero (propio) | Lista de citas. El barbero la usa para «Hoy», «Mañana» y «Próximos 7 días». | RF-AGE-02, RF-PBA-01 |
| GET | `/agenda/citas/{id}` | Admin; Barbero (propio) | Detalle: la cita (con su cliente, visitas e inasistencias, origen y nota), su `historial` de eventos (creada, confirmada, reprogramada…, con quién y cuándo) y sus `mensajes` de WhatsApp con su estado. Las acciones sobre la cita (`/reprogramar`, `/cancelar`, `/confirmar`, `/iniciar`, `/inasistencia`, `/recordatorio`) responden este mismo detalle actualizado. | RF-AGE-04 |
| POST | `/agenda/citas` | Admin | Cita creada por el personal: `{ cliente_id \| cliente_nuevo, servicio_id, barbero_id, inicio, nota }`; sin código ni anticipación mínima. Responde `201` con el detalle (como `GET /agenda/citas/{id}`). | RF-AGE-05 |
| GET | `/agenda/disponibilidad/horas?servicio_id=&barbero_id=&fecha=` | Admin | Variante para el personal: permite «ahora mismo» y fechas fuera de la ventana pública. | RF-AGE-05 |
| POST | `/agenda/citas/{id}/reprogramar` | Admin | `{ inicio, barbero_id }`; avisa al cliente. | RF-AGE-06, RF-NOT-04 |
| POST | `/agenda/citas/{id}/cancelar` | Admin | `{ motivo, avisar_cliente }`. | RF-AGE-07, RF-NOT-04 |
| POST | `/agenda/citas/{id}/confirmar` | Admin | Confirmación manual (p. ej., por llamada). | RN-31 |
| POST | `/agenda/citas/{id}/iniciar` | Admin; Barbero (propio) | Pasa a «En curso». | RF-AGE-10 |
| POST | `/agenda/citas/{id}/inasistencia` | Admin; Barbero (propio) | Solo el día de la cita o después; publica `InasistenciaMarcada`. | RF-AGE-08, RF-PBA-03 |
| POST | `/agenda/citas/{id}/recordatorio` | Admin; Barbero (propio) | Recordatorio manual por WhatsApp (plantilla `recordatorio_cita`, con botones). `409` si el cliente los desactivó o si ya se envió uno hace menos de 15 minutos. | RF-NOT-05, RF-PBA-11 |
| POST | `/agenda/conflictos` | Admin; Barbero (propio) | `{ barbero_id, horario_propuesto \| ausencia \| bloqueo }` → citas que quedarían afectadas, para mostrarlas antes de guardar. | RF-BAR-13, RF-PBA-09 |
| POST | `/agenda/citas/reasignacion` | Admin | `{ cita_ids, barbero_destino_id }`: misma hora, otro barbero; todo o nada; cada cliente recibe el aviso con un enlace nuevo. | RF-BAR-14 |
| POST | `/agenda/citas/aviso-reprogramacion` | Admin | `{ cita_ids }` → `{ avisadas }`: pide a los clientes elegir otra hora por WhatsApp, con un enlace nuevo. | RF-BAR-14 |
| GET | `/agenda/pendientes-de-cierre?fecha=` | Admin | Citas pasadas sin venta ni inasistencia. | RF-AGE-12 |
| GET / PUT | `/agenda/configuracion` | Admin | Reglas de agendamiento. | RF-CFG-02 |
| GET | `/agenda/resumen-barbero?barbero_id=` | Barbero (el suyo, aunque pida otro); Admin con `barbero_id` | «Mis citas»: `hoy`, `manana` y `proximos_7_dias` (de mañana al séptimo día, con sus totales y `dias_con_agenda_llena`). Cada día: `tipo_dia`, `estado` (`trabaja`, `no_trabaja`, `retirado` o el tipo de ausencia), citas por estado, `minutos_disponibles` (franjas − bloqueos), `minutos_ocupados`, `minutos_libres` (`null` si no trabaja) y `agenda_llena` (no cabe su servicio más corto, con las reglas del agendamiento). | RF-PBA-02 |

### 2.9 Ventas y caja (módulo `ventas`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/ventas/catalogos` | Admin; Barbero | Métodos de pago activos, descuentos (el barbero solo ve los permitidos a barberos), motivos y propinas rápidas. | RF-VEN-05, RF-VEN-07 |
| POST | `/ventas` | Admin; Barbero (propio) | Registra la venta (ver ejemplo en 3.2). Responde `201` con la venta completa, la misma de `GET /ventas/{id}`. Para el barbero, `barbero_id` se fija al suyo. | RF-VEN-01…09, RF-PBA-04 |
| GET | `/ventas?fecha=&desde=&hasta=&barbero_id=&cliente_id=&incluir_anuladas=&pagina=` | Admin; Barbero (propio) | Ventas (sin filtros: las de hoy; con `cliente_id`: su historial). | RF-VEN-10, RF-PBA-05 |
| GET | `/ventas/{id}` | Admin; Barbero (propio) | Detalle con líneas y pagos. | RF-VEN-01 |
| POST | `/ventas/{id}/anulacion` | Admin | `{ motivo }`; publica `VentaAnulada`. Si la venta era de una cita, la cita vuelve a «Confirmada» (evento `venta_anulada` en su historial) y se puede volver a cobrar: una cita nunca tiene dos ventas vigentes. | RF-VEN-12 |
| GET | `/ventas/caja?fecha=&barbero_id=` | Admin | Caja del día: total neto, número de ventas, propinas, desglose por método (propinas incluidas), anuladas y las ventas del día (filtrables por barbero). | RF-VEN-10 |
| GET | `/ventas/resumen-barbero?fecha=&barbero_id=` | Barbero (propio); Admin con `barbero_id` | «Lo que llevas hoy»: comisión + propinas y ventas netas. | RF-PBA-05 |
| GET | `/ventas/cierres?fecha=` | Admin | Cierre del día (`404` si no hay), con lo esperado por método, lo contado y la diferencia. | RF-VEN-11 |
| POST | `/ventas/cierres` | Admin | `{ fecha, contado: [{ metodo_pago_id, monto }], observacion }` → `201`. Lo esperado es una foto del momento; uno por día (`409 cierre_existente`). | RF-VEN-11 |
| GET | `/ventas/metodos-pago` | Admin | Todos los métodos, activos o no. | RF-CFG-07 |
| PATCH | `/ventas/metodos-pago/{id}` | Admin | `{ activo }`; siempre queda al menos uno activo. | RF-CFG-07 |
| GET / POST / PUT | `/ventas/descuentos`, `/ventas/motivos-descuento` | Admin | Configuración de descuentos y motivos (pendiente: Could). | RF-VEN-14 (Could) |

### 2.10 Reportes y dashboard (módulo `reportes`, solo lectura)

Periodo (RF-REP-01, RN-22): `?mes=AAAA-MM` o `?desde=&hasta=`; sin nada, el mes en curso hasta hoy. Un mes futuro, un rango que termina después de hoy, al revés o de más de un año responden `422` con el campo. Los PDF responden `application/pdf` como adjunto (`resumen-2026-11.pdf`, `Cache-Control: private, no-store`), quedan en la auditoría (RF-SIS-07) y, si Gotenberg no responde, `503 pdf_no_disponible` (los permisos se revisan antes). Tiempo máximo de estas rutas: 120 s. Detalle de cálculos en el [README del módulo](../../backend/modulos/reportes/README.md).

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/reportes/dashboard?fecha=` | Admin | Indicadores del día (hoy hasta la hora actual) con comparación frente al mismo día de la semana anterior a la misma hora, tendencia de 12 días, citas por estado y próximas, rendimiento y ocupación por barbero y ausentes. | RF-DAS-01…04 |
| GET | `/reportes/tendencias?mes=` | Admin | Ventas por servicio, clientes del mes (nuevos, recurrentes, retorno), clientes por mes frente al año anterior y horas pico de las últimas 4 semanas. | RF-DAS-05…07 |
| GET | `/reportes/alertas` | Admin | Citas de mañana sin confirmar, cumpleaños de la semana, ausencias, clientes por reactivar, solicitudes del titular abiertas y avisos no leídos. | RF-DAS-08 |
| GET | `/reportes/resumen` | Admin | Resumen de ventas y servicios: totales, por semana, por servicio, por método de pago y por barbero con comisión, propinas y total a pagar. | RF-REP-01, RF-REP-02 |
| GET | `/reportes/resumen.pdf` | Admin | El mismo reporte en PDF. | RF-REP-06 |
| GET | `/reportes/barberos/{id}` | Admin; Barbero (propio; el de otro, `404`) | Detallado y liquidación (cada venta con su porcentaje). | RF-REP-03, RF-PBA-06 |
| GET | `/reportes/barberos/{id}.pdf` | Admin; Barbero (propio) | PDF del detallado. | RF-REP-06 |
| GET | `/reportes/barberos.pdf` | Admin | Todos los barberos del periodo en un solo PDF, uno por página. | RF-REP-04 |
| GET | `/reportes/clientes` y `/reportes/clientes.pdf` | Admin | Reporte de clientes, los de más visitas y la lista para reactivar. | RF-REP-05 |
| GET | `/reportes/{tipo}.xlsx` | Admin | Exportación para contabilidad. *Pendiente (Could).* | RF-REP-07 |
| GET | `/reportes/servicios/{id}?mes=` | Admin | Vendidos e ingresos del servicio. *Pendiente (Could).* | RF-SER-07 |

### 2.11 Avisos, mensajes y auditoría (módulos `notificaciones` y `auditoria`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/avisos?no_leidos=true&pagina=` | Admin | Avisos internos: mensajes de citas que no se entregaron y citas afectadas por cambios de horario, bloqueos o ausencias (con `datos.cita_ids`). | RF-DAS-08, RF-NOT-07, RF-BAR-14 |
| POST | `/avisos/{id}/leido` | Admin | Marca como leído (idempotente). | RF-DAS-08 |
| GET | `/mensajes?destinatario=&estado=&canal=&plantilla=&pagina=` | Admin | Registro de mensajes y su estado, sin variables. Los de una cita salen en `GET /agenda/citas/{id}` (`mensajes`). | RF-NOT-07 |
| GET | `/auditoria?entidad=&entidad_id=&desde=&hasta=&pagina=` | Admin | Historial de cambios (p. ej., de un barbero). | RF-BAR-16, RF-SIS-07 |

### 2.12 Sistema

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/api/salud` | Proceso vivo (para Docker): `{"estado":"viva"}`. Fuera de `/api/v1`, como las dos siguientes. |
| GET | `/api/salud/lista` | Listo para atender: responde la base de datos (`503 {"estado":"sin_base_de_datos"}` si no). |
| GET | `/api/salud/operacion` | Para el monitor externo (RNF-OBS-02): `200 {"estado":"ok"}`, `503 {"estado":"alerta","alertas":[códigos]}` o `503 {"estado":"sin_base"}` (sin cifras ni datos personales). Códigos: `errores_del_servidor`, `api_lenta`, `worker_detenido`, `trabajos_atascados`, `correos_atrasados`, `correo_sin_configurar`, `whatsapp_atrasados`, `whatsapp_fallidos`. |
| GET | `/api/v1/sistema/estado` | Solo Admin: alertas con su mensaje, informativos (p. ej., WhatsApp sin proveedor), métricas de la API de este proceso (peticiones por clase, `5xx` y latencia p50/p95/p99 de los últimos 5 y 15 minutos y desde el arranque, rutas destacadas por su plantilla), cola de mensajes por canal (pendientes, atraso, enviados y fallidos) y trabajos (vencidos, atascados, minutos desde el último). |
| GET | `/media/publico/{clave}` | Fotos públicas, servidas por la API desde el volumen con caché de un año (claves aleatorias e inmutables). |
| GET | `/api/v1/archivos/{id}?exp=&firma=` | Descarga de un archivo privado con URL firmada (HMAC-SHA256) que vence en 5 minutos; siempre como adjunto. |

### 2.13 Solo en el ambiente de pruebas E2E

Solo existen con `DIAMANTE_PRUEBAS_E2E=true`, que `docker-compose.override.yml` y `docker-compose.e2e.yml` activan. En producción la configuración la rechaza y el proceso no arranca, así que la imagen de producción nunca los expone. Implementados: mensajes (fase 3), reloj y trabajos (fase 4); `reinicio` llega con el escenario base y Playwright.

El reloj vive en la API. En el ambiente E2E la API ejecuta también las tareas del worker (`DIAMANTE_WORKER_EN_API=true`), así todo el sistema comparte la hora simulada; en desarrollo el worker aparte sigue con la hora real.

| Método | Ruta | Descripción |
|---|---|---|
| POST | `/_pruebas/reinicio` | *Pendiente* (con el escenario base y Playwright). Vaciará los datos y cargará el escenario base. |
| GET / POST / DELETE | `/_pruebas/reloj` | Consulta, fija (`{ "ahora": "2026-11-18T14:50:00-05:00" }`; desde ahí el tiempo sigue corriendo) o devuelve a la hora real el reloj del servidor. |
| GET | `/_pruebas/mensajes?destinatario=3105559917` | Últimos 20 mensajes en cola para ese destinatario (plantilla y variables), por ejemplo para leer el código de verificación. |
| POST | `/_pruebas/trabajos/ejecutar` | Una pasada del worker en ese momento: webhooks, trabajos listos (recordatorios, mantenimiento), WhatsApp y correos. Devuelve el resumen. |

---

## 3. Ejemplos de contratos críticos

### 3.1 Crear una cita en línea

```http
POST /api/v1/publico/citas
Content-Type: application/json

{
  "comprobante": "b3JjLmRpYW1hbnRlLnZlcmlmaWNhY2lvbi4uLg",
  "servicio_id": 3,
  "barbero_id": null,
  "inicio": "2026-11-19T15:30:00-05:00",
  "cliente": { "nombre": "Andrés Felipe Mora", "correo": null, "fecha_nacimiento": null },
  "autoriza_tratamiento_datos": true,
  "politica_version_id": 1
}
```

```http
HTTP/1.1 201 Created
Location: /api/v1/publico/citas/gestion/Zk3v…

{
  "cita": {
    "id": 41,
    "estado": "confirmada",
    "origen": "en_linea",
    "inicio": "2026-11-19T15:30:00-05:00",
    "fin": "2026-11-19T16:45:00-05:00",
    "precio": 55000,
    "servicio": { "id": 3, "nombre": "Combo cabello + barba", "duracion_min": 75 },
    "barbero": { "id": 1, "nombre": "Santiago Ríos", "asignacion_automatica": true },
    "cancelacion_tardia": null,
    "motivo_cancelacion": null
  },
  "enlace_gestion": "https://diamante.co/cita/Zk3v…",
  "calendario_google": "https://calendar.google.com/calendar/render?action=TEMPLATE&…"
}
```

Reglas: el celular sale del comprobante (no del cuerpo); el comprobante se consume en la misma transacción que crea la cita; si la hora se tomó entre medio, la respuesta es `409 conflicto_horario` con `alternativas` y el comprobante sigue vigente para reintentar. La cita nace `confirmada`, salvo la de un cliente que superó el umbral de inasistencias con la política «confirmar por llamada», que nace `sin_confirmar` (con «bloquear la reserva en línea» responde `403 reserva_bloqueada`). La cita pública no trae `cliente` ni `nota`: solo las vistas del personal los incluyen.

### 3.2 Registrar una venta

```http
POST /api/v1/ventas
Content-Type: application/json

{
  "cita_id": 1842,
  "cliente": { "tipo": "registrado", "id": 57 },
  "barbero_id": 1,
  "servicio_ids": [3, 5],
  "descuento": { "descuento_id": 1, "motivo_descuento_id": 1 },
  "propina": 5000,
  "pagos": [{ "metodo_pago_id": 2, "monto": 65300 }]
}
```

`cliente` acepta tres formas: `{ "tipo": "registrado", "id": 57 }`, `{ "tipo": "nuevo", "nombre": "…", "celular": "…", "correo": null }` u `{ "tipo": "ocasional" }`. Con `cita_id`, el cliente es el de la cita (se ignora `cliente`) y, si no se indica `barbero_id`, también el barbero. `servicio_ids` va siempre: lo que de verdad se hizo, que puede diferir de lo agendado.

La respuesta es la venta completa, la misma de `GET /ventas/{id}` (y de la anulación, el listado y la caja): así el panel imprime el tiquete sin pedirla otra vez.

```http
HTTP/1.1 201 Created

{
  "id": 9031,
  "numero_ticket": 148,
  "registrada_en": "2026-11-19T16:52:10-05:00",
  "fecha_operacion": "2026-11-19",
  "registrada_por": 2,
  "estado": "registrada",
  "barbero": { "id": 1, "nombre": "Santiago Ríos" },
  "cliente": { "id": 57, "nombre": "Andrea Gómez" },
  "cita_id": 1842,
  "lineas": [
    { "posicion": 1, "servicio_id": 3, "nombre": "Corte clásico", "precio": 35000 },
    { "posicion": 2, "servicio_id": 5, "nombre": "Barba", "precio": 32000 }
  ],
  "subtotal": 67000,
  "descuento": {
    "descuento_id": 1, "nombre": "Cliente frecuente 10 %",
    "motivo_descuento_id": 1, "motivo": "Fidelización", "valor": 6700
  },
  "venta_neta": 60300,
  "propina": 5000,
  "total_cobrado": 65300,
  "porcentaje_comision": 50,
  "comision_barbero": 30150,
  "pagos": [{ "metodo_pago_id": 2, "nombre": "Nequi", "monto": 65300 }],
  "anulacion": null
}
```

- `cliente` es `null` en una venta a un cliente ocasional; `descuento`, `null` sin descuento; `anulacion`, `{ anulada_en, anulada_por, motivo }` si se anuló.
- La cita no viene en la respuesta, solo `cita_id`: su estado lo maneja `agenda` y, si la venta se registró, la cita quedó «Atendida» en la misma transacción.

Reglas: el servidor recalcula todos los valores (los del cliente no se usan); los servicios deben ser ofrecidos por el barbero; el barbero solo puede usar descuentos permitidos a barberos; si la suma de `pagos` no coincide con el total, responde `422 validacion`; una cita ya cobrada responde `409 cita_ya_cobrada`.

---

## 4. Eventos entre módulos

Los módulos no se llaman en círculo: cuando un módulo necesita reaccionar a lo que pasa en otro, se suscribe a un evento del crate `contratos`.

| Evento | Publica | Lo consumen | Efecto | Entrega |
|---|---|---|---|---|
| `CitaCreada` | agenda | (ninguno aún) | La confirmación por WhatsApp con enlace la encola la propia agenda con la fachada `BuzonSalida` en la misma transacción (*outbox*); el worker la envía | Misma transacción |
| `CitaReprogramada` / `CitaCancelada` | agenda | (ninguno aún) | Igual: el aviso al cliente se encola en la transacción del cambio | Misma transacción |
| `InasistenciaMarcada` | agenda | clientes | Suma inasistencias a la proyección | Misma transacción |
| `VentaRegistrada` | ventas | agenda, clientes | Cita «Atendida»; visitas, total gastado y barbero habitual | Misma transacción |
| `VentaAnulada` | ventas | agenda, clientes | La cita cobrada vuelve a «Confirmada» (se puede cobrar de nuevo); resta la visita y el gasto (la última visita vigente viene en el evento) | Misma transacción |
| `HorarioCambiado` / `BloqueoCreado` / `AusenciaRegistrada` | barberos | agenda | Si hay citas afectadas, crea un aviso para el administrador (el panel también las consulta antes de guardar con `POST /agenda/conflictos`) | Misma transacción |
| `BarberoRetirado` | barberos | agenda | Sus citas pendientes desde la fecha del retiro generan un aviso para reasignarlas o pedir otra hora (como una ausencia sin fin) | Misma transacción |
| `RespuestaWhatsAppRecibida` | notificaciones | agenda | «Confirmo» confirma la cita; «Cancelar cita» la cancela. Se publica al procesar el webhook en el worker, en la transacción que lo marca procesado | Cola |
| `AccesoRevocado` | barberos | (ninguno) | La cuenta la desactiva `barberos` por la fachada `CuentasPersonal` de `identidad`, en la misma transacción. La agenda no necesita reaccionar: lee el estado del barbero (retiro, ausencias, citas en línea) al calcular la disponibilidad | Misma transacción |
