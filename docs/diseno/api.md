# Diseño de la API REST

**Proyecto**: Diamante Barbería · **Backend**: Rust + Axum · **Versión del documento**: 1.0 (18/11/2026)

La API la consumen el frontend (Astro + React) y Meta (webhooks de WhatsApp). La especificación OpenAPI se genera desde el código (`utoipa`) y de ella salen los tipos TypeScript del frontend (`openapi-typescript`). Este documento define el contrato antes de construirlo.

---

## 1. Convenciones

| Tema | Convención |
|---|---|
| Base | `https://<dominio>/api/v1`. Caddy enruta `/api/*` al backend; el sitio y la API comparten dominio. |
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

| Estado | `codigo` | Cuándo |
|---|---|---|
| 400 | `solicitud_invalida` | JSON mal formado o parámetros imposibles de interpretar. |
| 401 | `no_autenticado`, `credenciales_invalidas` | Falta la sesión, o la sesión está vencida o revocada / correo o contraseña incorrectos. |
| 403 | `prohibido` / `origen_invalido` / `acceso_suspendido` | La política no lo permite / la verificación CSRF falló / la cuenta no tiene acceso al panel. |
| 404 | `no_encontrado` | El recurso no existe o no es del barbero. |
| 409 | `conflicto_horario`, `cita_ya_cobrada`, `estado_invalido`, `duplicado`, `correo_en_uso`, `ultimo_administrador`, `no_puede_desactivarse` | Reglas de concurrencia o de estado. |
| 410 | `enlace_vencido`, `codigo_vencido` | Token o código vencido. |
| 422 | `validacion` | Reglas de negocio o de formato; `errores` trae `[{ "campo": "celular", "mensaje": "…" }]`. |
| 429 | `demasiados_intentos` | Límite de intentos; incluye el encabezado `Retry-After`. |
| 500 | `error_interno` | Error inesperado; incluye un `id_solicitud` para rastrearlo en los logs. |

### 1.4 Límites de uso (por IP, salvo indicación)

| Grupo | Límite |
|---|---|
| `POST /publico/verificaciones` | 3 por celular cada 15 min y 10 por IP por hora |
| `POST /publico/verificaciones/{id}/confirmar` | 5 intentos por código (luego se invalida) |
| `POST /auth/sesion` | 10 por IP cada 15 min; la cuenta se bloquea 15 min tras 5 fallos |
| `POST /auth/recuperacion` | 3 por correo y 10 por IP por hora (siempre responde 202) |
| Resto de `/publico` | 120 por minuto |
| Panel | 600 por minuto por sesión |

---

## 2. Endpoints por módulo

### 2.1 Público — sitio y agendamiento (módulos `negocio`, `catalogo`, `barberos`, `agenda`, `verificacion`)

| Método | Ruta | Descripción | Requisitos |
|---|---|---|---|
| GET | `/publico/landing` | Datos de la landing en una sola llamada: negocio, horario de atención, servicios y combos visibles, barberos visibles con su estado («de vacaciones hasta…») y políticas vigentes. Respuesta con `Cache-Control: public, max-age=60`. | RF-PUB-01…04, RF-CFG-03 |
| GET | `/publico/servicios` | Servicios y combos agendables en línea que al menos un barbero ofrece. | RF-RES-02 |
| GET | `/publico/barberos?servicio_id=` | Opciones de barbero para el servicio, con su estado, motivo si no se puede elegir y próxima hora libre. | RF-RES-03, RF-RES-04 |
| GET | `/publico/disponibilidad/calendario?servicio_id=&barbero_id=&mes=2026-11` | Para cada día de la ventana: `disponible`, `ultimos_cupos` (≤ 3 horas) o `sin_cupos`. `barbero_id` vacío significa «Cualquier barbero». | RF-RES-05, RF-SIS-01 |
| GET | `/publico/disponibilidad/horas?servicio_id=&barbero_id=&fecha=` | Horas de inicio libres agrupadas en mañana, tarde y noche. Con «Cualquier barbero», cada hora trae el barbero que se asignaría. | RF-RES-05, RF-SIS-02 |
| POST | `/publico/verificaciones` | `{ "celular": "3105559917" }` → envía el código por WhatsApp. Responde `202` con `{ id, expira_en, reenviar_desde }`. | RF-RES-09 |
| POST | `/publico/verificaciones/{id}/confirmar` | `{ "codigo": "4821" }` → `{ comprobante, cliente }`. `cliente` trae nombre, correo y fecha de nacimiento si el celular ya existe: el autocompletado ocurre después de verificar. | RF-RES-08, RF-RES-09 |
| POST | `/publico/citas` | Crea la cita (ver ejemplo en 3.1). Responde `201`, o `409 conflicto_horario` con alternativas. | RF-RES-07, RF-RES-10, RF-RES-11, RF-NOT-01 |
| GET | `/publico/citas/gestion/{token}` | Ver la cita desde el enlace de WhatsApp: estado, servicio, barbero, fecha y hora, dirección, total y qué acciones permite. | RF-RES-12, RF-RES-14 |
| POST | `/publico/citas/gestion/{token}/cancelar` | Cancela hasta la hora de inicio; dentro del aviso mínimo queda marcada como tardía. | RF-RES-14, RN-09 |
| POST | `/publico/citas/gestion/{token}/reprogramar` | `{ "inicio": "…", "barbero_id": null }`. Mismas reglas de disponibilidad. | RF-RES-15 |
| PUT | `/publico/citas/gestion/{token}/recordatorios` | `{ "activos": false }`. | RF-RES-18 |
| GET | `/publico/citas/gestion/{token}/calendario.ics` | Descarga la cita en formato ICS. | RF-RES-17 (Could) |

### 2.2 Webhooks (módulo `notificaciones`)

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/webhooks/whatsapp` | Verificación de Meta (`hub.mode`, `hub.verify_token`, `hub.challenge`). |
| POST | `/webhooks/whatsapp` | Estados de entrega y respuestas de botones. Se guarda el evento y se responde `200` de inmediato; el worker lo procesa (p. ej., «Confirmo» → cita confirmada). |

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
| GET | `/barberos/exportacion.xlsx` | Admin | Exporta el listado (auditado). | RF-BAR-03 (Could) |
| POST | `/barberos` | Admin | Registra: datos esenciales, servicios, horario y visibilidad. Crea la cuenta y, si tiene acceso, envía la invitación. | RF-BAR-04, RF-BAR-07…09 |
| GET / PUT | `/barberos/{id}` | Admin | Ficha y edición de datos. | RF-BAR-04 |
| GET / PUT | `/barberos/{id}/datos-privados` | Admin | Dirección, RH, emergencia, seguridad social y cuenta de pago (campos cifrados). | RF-BAR-05 |
| PUT | `/barberos/{id}/visibilidad` | Admin | `{ mostrar_en_landing, recibe_citas_en_linea, acceso_panel }`. | RF-BAR-09, RF-AUT-08 |
| PUT | `/barberos/orden` | Admin | `{ barbero_ids: [...] }`. | RF-BAR-19 |
| GET / PUT | `/barberos/{id}/horario` | Admin; Barbero (propio) | Franjas por día; reemplaza el horario completo. Publica `HorarioCambiado` (el administrador recibe un aviso si hay citas afectadas). | RF-BAR-07, RF-PBA-09 |
| GET / PUT | `/barberos/{id}/servicios` | Admin; Barbero (propio) | `{ servicio_ids: [...] }`. | RF-BAR-08, RF-PBA-09 |
| POST | `/barberos/{id}/foto` | Admin | Multipart; imagen vertical. | RF-BAR-04 |
| GET / POST | `/barberos/{id}/documentos` | Admin | Lista y carga (multipart, con `tipo` y `periodo` para la PILA). | RF-BAR-06, RF-BAR-18 |
| GET | `/barberos/{id}/documentos/{doc_id}/descarga` | Admin | Redirige a una URL firmada de 5 minutos. | RNF-SEG-09 |
| DELETE | `/barberos/{id}/documentos/{doc_id}` | Admin | Quita el documento. | RF-BAR-06 |
| GET / POST | `/barberos/{id}/ausencias` | Admin | Vacaciones, incapacidad, licencia o suspensión con rango, observación y soporte. Publica `AusenciaRegistrada`. | RF-BAR-11, RF-BAR-12 |
| POST | `/barberos/{id}/ausencias/{aid}/anular` | Admin | Anula una ausencia. | RF-BAR-11 |
| POST | `/barberos/{id}/retiro` | Admin | `{ fecha, motivo }`; desactiva la cuenta y cierra sus sesiones. | RF-BAR-11, RF-BAR-15 |
| GET / POST | `/barberos/{id}/bloqueos` | Admin; Barbero (propio) | Bloqueos puntuales. | RF-PBA-10, RF-AGE-13 |
| DELETE | `/barberos/{id}/bloqueos/{bid}` | Admin; Barbero (propio) | Quita un bloqueo. | RF-PBA-10 |
| POST | `/barberos/{id}/solicitudes-correccion` | Barbero (propio) | `{ campo, valor_propuesto }`. | RF-PBA-08 (Could) |
| GET | `/barberos/solicitudes-correccion?estado=pendiente` | Admin | Solicitudes por atender. | RF-PBA-08 |
| PATCH | `/barberos/solicitudes-correccion/{id}` | Admin | `{ estado }`. | RF-PBA-08 |
| GET | `/yo` | Barbero | Su perfil en solo lectura, su estado y su próxima cita. | RF-PBA-07, RF-PBA-12 |

### 2.7 Clientes (módulo `clientes`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/clientes?q=&segmento=&pagina=` | Admin | Listado paginado con métricas, segmento y barbero habitual; indicadores de la base en `meta`. | RF-CLI-02, RF-CLI-04 |
| GET | `/clientes/buscar?q=` | Admin; Barbero | Búsqueda rápida por nombre o celular para la agenda y la venta (máximo 8 resultados, solo nombre, celular y segmento). | RF-VEN-03 |
| POST | `/clientes` | Admin; Barbero | `{ nombre, celular, correo }`, desde el panel. | RF-CLI-01 |
| GET | `/clientes/{id}` | Admin | Ficha: datos, métricas, autorizaciones y preferencias. Las últimas visitas salen de `GET /ventas?cliente_id=`. | RF-CLI-03 |
| PUT | `/clientes/{id}` | Admin | Edita datos y preferencias. | RF-CLI-07 |
| POST | `/clientes/{id}/anonimizacion` | Admin | Supresión por solicitud del titular; conserva las cifras. | RF-CLI-07, RNF-LEG-03 |
| GET | `/clientes/exportacion.xlsx` | Admin | Exportación (auditada). | RF-CLI-06 (Could) |
| GET / POST | `/clientes/solicitudes-titular` | Admin | Registrar y consultar solicitudes de habeas data. | RNF-LEG-03 |
| PATCH | `/clientes/solicitudes-titular/{id}` | Admin | `{ respuesta }`. | RNF-LEG-03 |
| GET / PUT | `/clientes/configuracion-segmentos` | Admin | Umbrales de los segmentos. | RF-CFG-08 |

### 2.8 Agenda (módulo `agenda`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/agenda/tablero?fecha=` | Admin | Vista de día: una columna por barbero con sus citas y los bloques no disponibles (fuera de turno, descanso, festivo, vacaciones). | RF-AGE-01, RF-AGE-03 |
| GET | `/agenda/citas?desde=&hasta=&barbero_id=&estado=` | Admin; Barbero (propio) | Lista de citas. El barbero la usa para «Hoy», «Mañana» y «Próximos 7 días». | RF-AGE-02, RF-PBA-01 |
| GET | `/agenda/citas/{id}` | Admin; Barbero (propio) | Detalle con historial del cliente, origen, nota y resultado si está cerrada. | RF-AGE-04 |
| POST | `/agenda/citas` | Admin | Cita creada por el personal: `{ cliente_id \| cliente_nuevo, servicio_id, barbero_id, inicio, nota }`; sin código ni anticipación mínima. | RF-AGE-05 |
| GET | `/agenda/disponibilidad/horas?servicio_id=&barbero_id=&fecha=&personal=true` | Admin | Variante para el personal (permite «ahora mismo»). | RF-AGE-05 |
| POST | `/agenda/citas/{id}/reprogramar` | Admin | `{ inicio, barbero_id }`; avisa al cliente. | RF-AGE-06, RF-NOT-04 |
| POST | `/agenda/citas/{id}/cancelar` | Admin | `{ motivo, avisar_cliente }`. | RF-AGE-07, RF-NOT-04 |
| POST | `/agenda/citas/{id}/confirmar` | Admin | Confirmación manual (p. ej., por llamada). | RN-31 |
| POST | `/agenda/citas/{id}/iniciar` | Admin; Barbero (propio) | Pasa a «En curso». | RF-AGE-10 |
| POST | `/agenda/citas/{id}/inasistencia` | Admin; Barbero (propio) | Solo el día de la cita o después; publica `InasistenciaMarcada`. | RF-AGE-08, RF-PBA-03 |
| POST | `/agenda/citas/{id}/recordatorio` | Admin; Barbero (propio) | Recordatorio manual por WhatsApp. | RF-NOT-05, RF-PBA-11 |
| POST | `/agenda/conflictos` | Admin; Barbero (propio) | `{ barbero_id, horario_propuesto \| ausencia \| bloqueo }` → citas que quedarían afectadas, para mostrarlas antes de guardar. | RF-BAR-13, RF-PBA-09 |
| POST | `/agenda/citas/reasignacion` | Admin | `{ cita_ids, barbero_destino_id }`. | RF-BAR-14 |
| POST | `/agenda/citas/aviso-reprogramacion` | Admin | `{ cita_ids }`: pide a los clientes reprogramar por WhatsApp. | RF-BAR-14 |
| GET | `/agenda/pendientes-de-cierre?fecha=` | Admin | Citas pasadas sin venta ni inasistencia. | RF-AGE-12 |
| GET / PUT | `/agenda/configuracion` | Admin | Reglas de agendamiento. | RF-CFG-02 |

### 2.9 Ventas y caja (módulo `ventas`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/ventas/catalogos` | Admin; Barbero | Métodos de pago activos, descuentos (el barbero solo ve los permitidos a barberos) y motivos. | RF-VEN-05, RF-VEN-07 |
| POST | `/ventas` | Admin; Barbero (propio) | Registra la venta (ver ejemplo en 3.2). Para el barbero, `barbero_id` se fija al suyo. | RF-VEN-01…09, RF-PBA-04 |
| GET | `/ventas?fecha=&desde=&hasta=&barbero_id=&cliente_id=&pagina=` | Admin; Barbero (propio) | Ventas (últimas del día, historial del cliente). | RF-VEN-10, RF-PBA-05 |
| GET | `/ventas/{id}` | Admin; Barbero (propio) | Detalle con líneas y pagos. | RF-VEN-01 |
| POST | `/ventas/{id}/anulacion` | Admin | `{ motivo }`; publica `VentaAnulada`. | RF-VEN-12 |
| GET | `/ventas/caja?fecha=` | Admin | Caja del día: total neto, número de ventas, propinas y desglose por método. | RF-VEN-10 |
| GET | `/ventas/resumen-barbero?fecha=` | Barbero (propio) | «Lo que llevas hoy»: comisión + propinas y ventas netas. | RF-PBA-05 |
| GET | `/ventas/cierres?fecha=` | Admin | Cierre del día si existe, con lo esperado por método. | RF-VEN-11 |
| POST | `/ventas/cierres` | Admin | `{ fecha, contado: [{ metodo_pago_id, monto }], observacion }`. | RF-VEN-11 |
| PATCH | `/ventas/metodos-pago/{id}` | Admin | `{ activo }`. | RF-CFG-07 |
| GET / POST / PUT | `/ventas/descuentos`, `/ventas/motivos-descuento` | Admin | Configuración de descuentos y motivos. | RF-VEN-14 (Could) |

### 2.10 Reportes y dashboard (módulo `reportes`, solo lectura)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/reportes/dashboard?fecha=` | Admin | Indicadores del día con comparación, tendencia de 12 días, citas por estado y próximas, rendimiento y ocupación por barbero. | RF-DAS-01…04 |
| GET | `/reportes/tendencias?mes=` | Admin | Ventas por servicio, clientes del mes, clientes por mes frente al año anterior y horas pico. | RF-DAS-05…07 |
| GET | `/reportes/alertas` | Admin | Citas de mañana sin confirmar, cumpleaños de la semana, ausencias, clientes por reactivar y avisos no leídos. | RF-DAS-08 |
| GET | `/reportes/resumen?desde=&hasta=` | Admin | Resumen de ventas y servicios (JSON para la vista previa). | RF-REP-01, RF-REP-02 |
| GET | `/reportes/resumen.pdf?desde=&hasta=` | Admin | El mismo reporte en PDF. | RF-REP-06 |
| GET | `/reportes/barberos/{id}?desde=&hasta=` | Admin; Barbero (propio) | Detallado y liquidación. | RF-REP-03, RF-PBA-06 |
| GET | `/reportes/barberos/{id}.pdf?desde=&hasta=` | Admin; Barbero (propio) | PDF del detallado. | RF-REP-06 |
| GET | `/reportes/barberos.pdf?desde=&hasta=` | Admin | Todos los barberos en un solo PDF, uno por sección. | RF-REP-04 |
| GET | `/reportes/clientes?desde=&hasta=` y `.pdf` | Admin | Reporte de clientes y lista para reactivar. | RF-REP-05 |
| GET | `/reportes/{tipo}.xlsx?desde=&hasta=` | Admin | Exportación para contabilidad. | RF-REP-07 (Could) |
| GET | `/reportes/servicios/{id}?mes=` | Admin | Vendidos e ingresos del servicio. | RF-SER-07 (Could) |

### 2.11 Avisos, mensajes y auditoría (módulos `notificaciones` y `auditoria`)

| Método | Ruta | Perfil | Descripción | Requisitos |
|---|---|---|---|---|
| GET | `/avisos?no_leidos=true` | Admin | Avisos internos. | RF-DAS-08 |
| POST | `/avisos/{id}/leido` | Admin | Marca como leído. | RF-DAS-08 |
| GET | `/mensajes?cita_id=&destinatario=&estado=` | Admin | Registro de mensajes y su estado. | RF-NOT-07 |
| GET | `/auditoria?entidad=&entidad_id=&desde=&hasta=&pagina=` | Admin | Historial de cambios (p. ej., de un barbero). | RF-BAR-16, RF-SIS-07 |

### 2.12 Sistema

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/salud` | Proceso vivo (para Docker). |
| GET | `/salud/lista` | Listo para atender: responde la base de datos. |
| GET | `/media/publico/{clave}` | Fotos públicas. Las sirve Caddy directamente desde el volumen, sin pasar por la API. |
| GET | `/archivos/{id}?exp=&firma=` | Descarga de un archivo privado con URL firmada (HMAC) y vencimiento. |

### 2.13 Solo en el ambiente de pruebas E2E

Se compilan únicamente con la *feature* de Cargo `pruebas-e2e`; la imagen de producción no los contiene.

| Método | Ruta | Descripción |
|---|---|---|
| POST | `/_pruebas/reinicio` | Vacía los datos y carga el escenario base. |
| POST | `/_pruebas/reloj` | `{ "ahora": "2026-11-18T14:50:00-05:00" }`: fija el reloj del servidor. |
| GET | `/_pruebas/mensajes?destinatario=` | Mensajes que el WhatsApp simulado «envió» (por ejemplo, para leer el código). |
| POST | `/_pruebas/trabajos/ejecutar` | Ejecuta en el momento las tareas pendientes del worker (recordatorios). |

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
    "estado": "confirmada",
    "servicio": { "id": 3, "nombre": "Combo cabello + barba", "duracion_min": 75 },
    "barbero": { "id": 1, "nombre": "Santiago Ríos", "especialidad": "Fades y degradados", "asignacion_automatica": true },
    "inicio": "2026-11-19T15:30:00-05:00",
    "fin": "2026-11-19T16:45:00-05:00",
    "total_a_pagar": 55000
  },
  "enlace_gestion": "https://diamante.co/cita/Zk3v…",
  "calendario_google": "https://calendar.google.com/calendar/render?action=TEMPLATE&…"
}
```

Reglas: el celular sale del comprobante (no del cuerpo); el comprobante se consume en la misma transacción que crea la cita; si la hora se tomó entre medio, la respuesta es `409 conflicto_horario` con `alternativas` y el comprobante sigue vigente para reintentar.

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

`cliente` acepta tres formas: `{ "tipo": "registrado", "id": 57 }`, `{ "tipo": "nuevo", "nombre": "…", "celular": "…", "correo": null }` u `{ "tipo": "ocasional" }`.

```http
HTTP/1.1 201 Created

{
  "id": 9031,
  "numero_ticket": 148,
  "subtotal": 67000,
  "descuento": 6700,
  "venta_neta": 60300,
  "propina": 5000,
  "total_cobrado": 65300,
  "comision_barbero": 30150,
  "cita": { "id": 1842, "estado": "atendida" }
}
```

Reglas: el servidor recalcula todos los valores (los del cliente no se usan); los servicios deben ser ofrecidos por el barbero; el barbero solo puede usar descuentos permitidos a barberos; si la suma de `pagos` no coincide con el total, responde `422 validacion`; una cita ya cobrada responde `409 cita_ya_cobrada`.

---

## 4. Eventos entre módulos

Los módulos no se llaman en círculo: cuando un módulo necesita reaccionar a lo que pasa en otro, se suscribe a un evento del crate `contratos`.

| Evento | Publica | Lo consumen | Efecto | Entrega |
|---|---|---|---|---|
| `CitaCreada` | agenda | notificaciones | Confirmación por WhatsApp con enlace | Cola (asíncrona) |
| `CitaReprogramada` / `CitaCancelada` | agenda | notificaciones | Aviso al cliente | Cola |
| `InasistenciaMarcada` | agenda | clientes | Suma inasistencias a la proyección | Misma transacción |
| `VentaRegistrada` | ventas | agenda, clientes | Cita «Atendida»; visitas, total gastado y barbero habitual | Misma transacción |
| `VentaAnulada` | ventas | clientes | Recalcula las métricas del cliente | Misma transacción |
| `HorarioCambiado` / `BloqueoCreado` / `AusenciaRegistrada` | barberos | agenda | Si hay citas afectadas, crea un aviso para el administrador | Misma transacción |
| `RespuestaWhatsAppRecibida` | notificaciones | agenda | «Confirmo» confirma la cita; «Cancelar» la cancela | Cola |
| `BarberoRetirado` / `AccesoRevocado` | barberos | identidad | Desactiva la cuenta y cierra sesiones | Misma transacción |
