# Módulo `agenda`

El corazón del producto: disponibilidad, asignación de barbero, citas, gestión por enlace,
tablero del panel y la landing pública (RF-PUB, RF-RES, RF-AGE, RF-CFG-02).

| | |
|---|---|
| Fase del plan | 3 ✅ · 4 (recordatorios, botones de WhatsApp y avisos) ✅ · 5 (cita «Atendida» al cobrar) ✅ |
| Esquema de base de datos | `agenda` (ver `migraciones/`); `cita_sin_solape` (EXCLUDE) garantiza que dos citas vigentes de un barbero nunca se cruzan (RNF-INT-01) |
| Fachadas que usa | `negocio` (datos públicos y calendario), `catalogo` (servicios), `barberos` (`EquipoAgenda`: franjas, ausencias, bloqueos y retiro), `clientes`, `verificacion` (comprobantes) y `notificaciones` (WhatsApp, avisos y estado de mensajes) |
| Fachadas que ofrece | `CitasParaVentas` (la cita que se cobra, bloqueada) y `CitasDeClientes` (citas por atender de un cliente, para `clientes` por un adaptador en `app`) |

## Motor de disponibilidad (`dominio/disponibilidad.rs`)

Funciones puras, probadas sin base de datos:

- Horas de inicio cada `intervalo_min` (30 o 15) dentro de las franjas del barbero para ese día
  de la semana, donde quepa la duración completa del servicio.
- Público: desde ahora + `anticipacion_minima_min` (120 por defecto) y hasta
  `ventana_reserva_dias` (30). Personal (panel): permite la hora en curso y fechas fuera de la
  ventana.
- Días especiales del negocio: **festivo** usa las franjas del domingo, **cerrado** no tiene
  horas y **cierre temprano** recorta el día.
- Ausencias (vacaciones, incapacidad, suspensión), retiro, bloqueos y citas vigentes ocupan.
- «Cualquier barbero»: para cada hora, el primero libre en el orden del equipo.
- Calendario: `sin_cupos` (0), `ultimos_cupos` (1 a 3), `disponible` o `fuera_de_ventana`.

## Endpoints (`/api/v1`)

### Públicos

| Método | Ruta | Notas |
|---|---|---|
| GET | `/publico/landing` | Negocio, horario calculado desde los barberos, servicios, equipo (vacaciones) y políticas. `Cache-Control: public, max-age=60` (RF-PUB-04) |
| GET | `/publico/servicios` | Los que se agendan en línea y algún barbero ofrece |
| GET | `/publico/barberos?servicio_id=` | Opciones con `disponible`, `motivo` («No realiza este servicio», «De vacaciones hasta…») y `proxima_hora` |
| GET | `/publico/disponibilidad/calendario?servicio_id=&barbero_id=&mes=AAAA-MM` | Estado de cada día |
| GET | `/publico/disponibilidad/horas?servicio_id=&barbero_id=&fecha=` | Horas libres en mañana, tarde y noche |
| POST | `/publico/citas` | Con el comprobante de `verificacion`. `201` + `Location`; `409 conflicto_horario` con `alternativas` (las 3 horas más cercanas); 20 por IP cada hora |
| GET | `/publico/citas/gestion/{token}` | Ver la cita del enlace (`private, no-store`) |
| POST | `/publico/citas/gestion/{token}/cancelar` | Antes de empezar; tardía si falta menos que `aviso_cancelacion_min` (RN-09) |
| POST | `/publico/citas/gestion/{token}/reprogramar` | `{ inicio, barbero_id }` |
| PUT | `/publico/citas/gestion/{token}/recordatorios` | `{ activos }` |

### Panel

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/agenda/tablero?fecha=` | Administrador | Columna por barbero: estado del día, franjas, bloqueos y citas |
| GET | `/agenda/citas?desde=&hasta=&barbero_id=&estado=` | Personal | El barbero solo ve las suyas |
| POST | `/agenda/citas` | Administrador | `cliente_id` o `cliente_nuevo`; queda «Sin confirmar», origen panel (RN-31) |
| GET | `/agenda/citas/{id}` | Personal | Con historial de eventos y el estado de sus WhatsApp (`mensajes`) |
| POST | `/agenda/citas/{id}/reprogramar` | Administrador | Otra hora y/o barbero; renueva el enlace y avisa al cliente |
| POST | `/agenda/citas/{id}/cancelar` | Administrador | `{ motivo: solicitud_cliente \| barbero_no_disponible \| otro, avisar_cliente }` |
| POST | `/agenda/citas/{id}/confirmar` | Administrador | Solo «Sin confirmar» |
| POST | `/agenda/citas/{id}/iniciar` | Personal (la propia) | Pasa a «En curso» |
| POST | `/agenda/citas/{id}/inasistencia` | Personal (la propia) | Solo el día de la cita o después; suma en la ficha del cliente |
| POST | `/agenda/citas/{id}/recordatorio` | Personal (la propia) | Recordatorio manual (RF-NOT-05): no si el cliente los desactivó, máximo uno cada 15 minutos |
| POST | `/agenda/citas/reasignacion` | Administrador | `{ cita_ids, barbero_destino_id }`: misma hora, otro barbero; todo o nada (RF-BAR-14) |
| POST | `/agenda/citas/aviso-reprogramacion` | Administrador | `{ cita_ids }`: WhatsApp con un enlace nuevo para elegir otra hora (RF-BAR-14) |
| GET | `/agenda/disponibilidad/horas?servicio_id=&barbero_id=&fecha=` | Administrador | Variante del personal del motor |
| POST | `/agenda/conflictos` | Administrador o el propio barbero | Citas afectadas por una `ausencia`, un `bloqueo` o un `horario_propuesto` (RF-BAR-13) |
| GET | `/agenda/pendientes-de-cierre?fecha=` | Administrador | Citas ya terminadas sin cerrar |
| GET/PUT | `/agenda/configuracion` | Administrador | Reglas de agendamiento con las opciones del mockup; auditado |

## Reglas clave

- La reserva en línea ocurre en **una transacción**: consume el comprobante, valida la política
  de datos vigente, aplica la política de inasistencias (RN-10: `confirmar_por_llamada` deja la
  cita «Sin confirmar», `bloquear_reserva_en_linea` la rechaza), registra al cliente, inserta la
  cita, crea el enlace de gestión, encola el WhatsApp de confirmación y publica `CitaCreada`.
  Si algo falla, nada queda (ni el comprobante gastado).
- El enlace de gestión es un token de 256 bits guardado como SHA-256; vence 30 días después de
  la cita y se renueva al reprogramar desde el panel.
- Precio vigente al agendar; las cancelaciones nunca suman inasistencias (DA-17).

## Recordatorios y respuestas (fase 4)

- **Automáticos** (tarea `agenda.recordatorios`, cada 5 minutos): el de 24 h sale desde un día
  antes y hasta 2 h antes; el de 2 h, desde 2 h antes hasta la hora. Solo si la cita ya tenía su
  hora cuando el recordatorio venció (una cita agendada hace una hora no recibe el de 24 h) y
  el cliente no los desactivó. Un índice único deja uno solo por cita, tipo y hora (E2E-NOT-01).
- **Botones del recordatorio**: «Confirmo» confirma una cita «Sin confirmar»; «Cancelar cita»
  la cancela como desde el enlace (tardía según el aviso mínimo) y envía el aviso.
- **Avisos a la administradora**: si un cambio de horario, un bloqueo o una ausencia de un
  barbero deja citas por fuera, se crea un aviso con las citas afectadas (RF-BAR-14, DA-08).

## Eventos que publica (crate `contratos`)

- `CitaCreada`, `CitaReprogramada`, `CitaCancelada`, `InasistenciaMarcada`

## Eventos que consume

- `VentaRegistrada` (ventas): la cita queda «Atendida», con el evento en su historial.
- `RespuestaWhatsAppRecibida` (notificaciones): confirma o cancela la cita.
- `HorarioCambiado`, `BloqueoCreado`, `AusenciaRegistrada` (barberos): aviso si hay citas afectadas.

## Estructura

```text
agenda/src/
├── api.rs                 # re-exportaciones públicas (DTO)
├── modulo.rs              # DependenciasAgenda y ModuloAgenda::rutas()
├── dominio/               # disponibilidad (motor), cita (estados) y configuración
├── aplicacion/            # publico, reserva, gestion y panel sobre Puertos<C>
├── infraestructura/       # RepositorioAgendaPg
└── http/                  # rutas públicas y del panel; 409 con `alternativas`
```
