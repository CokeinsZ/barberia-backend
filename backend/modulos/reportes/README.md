# Módulo `reportes`

Dashboard, resumen de ventas, liquidación por barbero, reporte de clientes y sus PDF (RF-DAS-01
a 08, RF-REP-01 a 06, RF-PBA-06). Solo lectura: no tiene esquema propio y lee las **vistas
publicadas** (`v_`) de los demás módulos (regla 4 del plan). Lo único que escribe es la
auditoría de cada PDF descargado (RF-SIS-07).

| | |
|---|---|
| Fase del plan | 6 ✅ |
| Esquema de base de datos | — (lee `ventas.v_venta`, `ventas.v_venta_linea`, `ventas.v_venta_pago`, `agenda.v_cita`, `catalogo.v_servicio`, `barberos.v_barbero` y `clientes.v_cliente_resumen`) |
| Fachadas que usa | `barberos` (`EquipoAgenda`: franjas, ausencias y bloqueos para la ocupación), `negocio` (`CalendarioNegocio` para festivos y cierres; `ParametrosNegocio` para el nombre y el NIT del PDF), `clientes` (días de inactividad y solicitudes del titular abiertas), `notificaciones` (avisos sin leer) y `auditoria` |
| Servicio externo | Gotenberg 8 (HTML → PDF con Chromium), `DIAMANTE_GOTENBERG_URL` |

## Endpoints (`/api/v1`)

Periodo (RF-REP-01, RN-22): `?mes=AAAA-MM` o `?desde=&hasta=`; sin nada, el mes en curso hasta
hoy. Un mes futuro, un rango que termina después de hoy, un rango al revés o de más de un año
responden `422` con el campo. Los parámetros desconocidos responden `400`.

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/reportes/dashboard?fecha=` | Administrador | Hoy (por defecto) o un día pasado. Ver «Dashboard» |
| GET | `/reportes/tendencias?mes=` | Administrador | Por servicio, clientes del mes, clientes por mes (año actual y anterior) y horas pico |
| GET | `/reportes/alertas` | Administrador | Ver «Alertas» |
| GET | `/reportes/resumen` | Administrador | Totales, ventas netas por semana (lunes a domingo), por servicio, por método de pago y por barbero con su liquidación |
| GET | `/reportes/resumen.pdf` | Administrador | El mismo reporte en PDF |
| GET | `/reportes/barberos/{id}` | Administrador; el barbero, el suyo («Mi reporte») | Vinculación, comisión vigente, indicadores, servicios, liquidación y cada venta. El de otro barbero responde `404` |
| GET | `/reportes/barberos/{id}.pdf` | Igual | PDF del anterior |
| GET | `/reportes/barberos.pdf` | Administrador | Todos los barberos del periodo, cada uno en su página (RF-REP-04) |
| GET | `/reportes/clientes` y `.pdf` | Administrador | Atendidos, nuevos, recurrentes, retorno, ocasionales, inasistencias, los 10 con más visitas y los 100 primeros por reactivar (con el total) |

PDF: `Content-Type: application/pdf`, `Content-Disposition: attachment; filename="resumen-2026-11.pdf"`
(el nombre lleva el mes o el rango) y `Cache-Control: private, no-store`. Si Gotenberg no está
configurado o falla, `503 pdf_no_disponible` (se puede reintentar); los permisos se revisan
antes, así que un barbero recibe `403` o `404` aunque Gotenberg no esté. Cada PDF generado
queda en la auditoría (`entidad = reporte`, acción `exportar_reporte`, con el periodo y el
tamaño). Estas rutas tienen 120 s de tiempo máximo (el resto de la API, 30 s).

## Reglas de cálculo (RN-15, RN-21, DA-09)

- **Solo ventas vigentes**: las anuladas no cuentan en nada (las vistas filtran `estado = 'registrada'`).
- **Clientes atendidos** = ventas registradas (los ocasionales cuentan). **Ticket promedio** =
  ventas netas ÷ clientes atendidos, al peso (las mitades suben).
- **Comisión**: la suma de la comisión de cada venta con el porcentaje que tenía el barbero al
  venderse (`v_venta.comision`). Cambiar el porcentaje no cambia lo ya vendido.
- **Propinas**: 100 % del barbero, sin importar el método de pago. **Total a pagar** = comisión + propinas.
- **Por método de pago**: lo cobrado por cada método, propinas incluidas (cuadra con el total cobrado).
- **Por servicio**: unidades e ingresos al precio de cada línea, antes de descuentos.
- **Nuevos**: su primera venta (de siempre) cayó en el periodo. **Recurrentes** = atendidos − nuevos.
- **Retorno** (RN-21): atendidos que, en alguna visita del periodo, volvían en menos de 60 días
  desde la anterior (dos tiquetes el mismo día no son un retorno). Porcentaje con un decimal.
- **Por reactivar**: última visita hace más de `dias_inactividad` días (configuración de
  segmentos de `clientes`, 60 por defecto), con su último servicio; los anonimizados no aparecen.

## Dashboard (RF-DAS-01 a 04)

- `hoy`: totales del día (para hoy, hasta la hora actual). `comparacion`: el mismo día de la
  semana anterior **hasta la misma hora** (para un día pasado, completo) y la variación en
  porcentaje (`null` si la semana anterior fue cero).
- `tendencia`: ventas netas de los últimos 12 días, el consultado incluido.
- `citas`: atendidas, en curso, próximas (sin confirmar o confirmadas que no han empezado), sin
  cerrar (ya pasó su hora y no tienen venta ni inasistencia), no asistió y canceladas; y las
  próximas 10 con hora, cliente, servicio y barbero.
- `barberos`: por barbero, estado (`activo`, el tipo de ausencia o `retirado` si vendió ese día y ya no está en el equipo: la tabla siempre suma el total del día), clientes, servicios, ventas
  netas, propinas y **ocupación** = minutos con citas (sin confirmar, confirmadas, en curso o
  atendidas) ÷ minutos de sus franjas del día, descontando bloqueos; los festivos usan las
  franjas del domingo, los días cerrados no tienen franjas y el cierre temprano las recorta (las
  mismas reglas del agendamiento). `ausentes`: quienes tienen una ausencia ese día.

## Alertas (RF-DAS-08)

Citas de mañana sin confirmar, cumpleaños de los próximos 7 días (quien nació un 29 de febrero
lo celebra el 28 en los años no bisiestos), barberos ausentes hoy, cuántos clientes hay por
reactivar, solicitudes del titular sin responder (consultas, correcciones y supresiones) y
avisos internos sin leer (cambios de horario, bloqueos o ausencias con citas afectadas y
mensajes fallidos, que se listan en `/avisos`).

## PDF (RF-REP-06)

A4 (márgenes de 2,2 cm arriba, 1,9 cm abajo y 1,4 cm a los lados). El encabezado de cada página lleva la marca, el tipo de reporte,
el periodo y la fecha y hora de generación; el pie, el nombre, el NIT (o «NIT sin registrar») y
«Página X de Y». Las tablas repiten sus títulos en cada página y todo dato de la base se escapa.
El HTML se arma en `aplicacion/pdf.rs` y Gotenberg lo convierte
(`POST /forms/chromium/convert/html` con `index.html`, `header.html` y `footer.html`).

En Docker Compose, Gotenberg corre sin puertos publicados, con Chromium **sin JavaScript** y
**sin red** (`--chromium-allow-list=^file:///tmp/.*`): aunque un dato llegara sin escapar, no
podría cargar nada externo ni de la red interna. Medido con Gotenberg 8.40 (RNF-REN-04): un mes
de un barbero con 300 ventas (14 páginas), de 0,4 a 3,6 s; los 6 barberos con 1.800 ventas (84
páginas), de 1,3 a 12,4 s según la carga del equipo (meta: 30 s).

## Eventos

No publica ni consume eventos.

## Estructura

```text
reportes/src/
├── api.rs                 # DTO públicos y ErrorReportes
├── modulo.rs              # DependenciasReportes, rutas() y genera_pdf()
├── dominio/               # periodo (mes o rango, semanas), cálculos (ticket, porcentajes, ocupación) y formato es-CO
├── aplicacion/            # casos de uso, DTO, puertos (lecturas y GeneradorPdf) y HTML de los PDF
├── infraestructura/       # LecturasPg (solo vistas publicadas) y Gotenberg (reqwest multipart)
└── http/                  # rutas del panel
```

## Pruebas

- Unitarias: periodo, semanas y textos; ticket, porcentajes simétricos, promedios y ocupación;
  formato de pesos, números y decimales; HTML escapado, encabezado y pie; 29 de febrero.
- `crates/app/src/pruebas_reportes.rs` (PostgreSQL real, todas las migraciones, Gotenberg de
  mentira que devuelve lo que recibe): siembra agosto, octubre y noviembre por la API moviendo
  el reloj y comprueba cada cifra del dashboard, el resumen, la liquidación (E2E-REP-02, con
  el porcentaje de cada venta), clientes, tendencias, alertas, los PDF (E2E-REP-03, con el
  periodo en el encabezado y la auditoría), Gotenberg caído (503 sin auditoría), los periodos
  inválidos y los permisos del barbero (E2E-REP-04).
- `infraestructura/gotenberg.rs`: con `PRUEBAS_GOTENBERG_URL=http://127.0.0.1:3000` genera los
  cuatro PDF reales con el volumen de un mes intenso, mide los tiempos contra RNF-REN-04 y, con
  `PRUEBAS_PDF_DIR`, los guarda para revisarlos con `pdfinfo` y `pdftotext`.
