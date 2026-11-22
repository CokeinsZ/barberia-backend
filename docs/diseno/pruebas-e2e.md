# Diseño de las pruebas de extremo a extremo (E2E)

**Proyecto**: Diamante Barbería · **Herramienta**: Playwright (TypeScript) · **Versión del documento**: 1.0 (18/11/2026)

---

## 1. Objetivo y alcance

Las pruebas E2E verifican los recorridos completos de cada perfil en un navegador real, contra el sistema levantado con Docker Compose (frontend, API, worker, PostgreSQL, Gotenberg). No reemplazan a las demás pruebas, las coronan:

| Nivel | Qué cubre | Dónde vive | Cantidad |
|---|---|---|---|
| Unitarias | Reglas de dominio sin infraestructura: disponibilidad, totales de venta, liquidación, segmentos, estados | Dentro de cada módulo del backend y del frontend | Muchas (cientos) |
| Integración | Casos de uso con PostgreSQL real, repositorios, restricciones y endpoints | `backend/modulos/<modulo>/pruebas/` | Decenas por módulo |
| **E2E** | **Recorridos críticos de punta a punta, incluidos WhatsApp, correo, PDF y tiempo** | **`e2e/`** | **66 escenarios (sección 6)** |

Criterio para que algo sea E2E: el valor está en la integración de varias piezas (navegador + API + base de datos + worker + mensajería). Las combinaciones de reglas se prueban en niveles inferiores, que son más rápidos.

## 2. Herramientas

| Herramienta | Uso |
|---|---|
| Playwright Test | Ejecución, aserciones con espera automática, trazas y video al fallar, reporte HTML. |
| Proyectos de Playwright | `escritorio-chrome` (1280 × 800), `movil-safari` (iPhone, WebKit) y `movil-chrome` (Pixel). Los escenarios del panel corren en escritorio y móvil; los públicos, sobre todo en móvil. |
| `page.clock` de Playwright | Fija la hora del navegador; junto con `/_pruebas/reloj` del backend, todo el sistema vive en la misma hora simulada. |
| `@axe-core/playwright` | Revisión automática de accesibilidad (WCAG 2.2 AA). |
| WhatsApp simulado | Adaptador del backend que, en el ambiente E2E, guarda los mensajes en lugar de enviarlos a Meta; se consultan con `GET /_pruebas/mensajes`. |
| Mailpit | Captura los correos (invitaciones y recuperación) y los expone por su API para leer los enlaces. |
| Webhooks firmados | Las pruebas firman con el secreto de prueba los eventos que enviaría Meta (respuestas «Confirmo» y estados de entrega). |

## 3. Ambiente de pruebas

```bash
docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d --wait
npx playwright test --project=movil-chrome   # o sin filtro para todos
```

`docker-compose.e2e.yml` cambia, respecto a producción:

- La API arranca con `DIAMANTE_PRUEBAS_E2E=true` (endpoints `/_pruebas/*`; prohibido en producción) y ejecuta también las tareas del worker (`DIAMANTE_WORKER_EN_API=true`), para que todo el sistema comparta el reloj de pruebas. El WhatsApp es el simulado (siempre fuera de producción): los celulares terminados en `0000` fallan temporalmente y los terminados en `9999` de forma permanente (E2E-NOT-04). Los webhooks se firman con `DIAMANTE_WHATSAPP_SECRETO_APP=secreto-de-pruebas-whatsapp`.
- PostgreSQL usa `tmpfs`: la base de datos vive en memoria y es rápida y desechable.
- Se agrega Mailpit como servidor SMTP.
- La caché de la landing dura 2 segundos, en lugar de 60, para probar RF-PUB-04 sin esperas largas.
- La API responde directamente en `http://localhost:8080` (en producción va detrás del Traefik de Dokploy).

### 3.1 Datos y reloj

- **Reloj fijo**: miércoles 18 de noviembre de 2026, 2:50 p. m. (America/Bogota), la misma hora de demostración del mockup.
- **Escenario base** (`POST /_pruebas/reinicio`), con los datos del mockup:
  - **Barberos**: Santiago (con cuenta de barbero), Mateo, Julián y Valentina activos; Andrés de vacaciones del 16 al 30 de noviembre; David retirado.
  - **Servicios**: Corte, Barba, Combo cabello + barba (combo), Afeitado, Cejas y Corte infantil (inactivo).
  - **Clientes**: Andrés Felipe Mora (310 555 9917, 12 visitas) y Jorge Patiño (2 inasistencias), entre otros.
  - **Agenda y ventas de hoy**: según el mockup, incluida una cita en curso.
  - **Cuentas**: la administradora (`admin@diamante.test`) y Santiago (`santiago@diamante.test`), con contraseñas de prueba.
- Cada archivo de pruebas empieza con `reinicio` + `reloj`. Las pruebas no dependen del orden ni de datos que haya dejado otra prueba.

### 3.2 Paralelismo

Se empieza con **un worker**: todo el sistema comparte una base de datos y un reloj, y así las pruebas son deterministas. La meta es que la suite completa tarde menos de 15 minutos. Si crece, se paraleliza levantando un proyecto de Compose aislado por worker (`COMPOSE_PROJECT_NAME=e2e-<n>`).

## 4. Organización del código de pruebas

Misma filosofía modular del resto del monorepo: una carpeta por módulo funcional, y las piezas reutilizables en carpetas separadas.

```text
e2e/
├── playwright.config.ts
├── package.json
├── pruebas/                      # especificaciones, una carpeta por módulo
│   ├── publico/landing.spec.ts
│   ├── agendamiento/reserva.spec.ts
│   ├── agendamiento/gestion-cita.spec.ts
│   ├── notificaciones/whatsapp.spec.ts
│   ├── acceso/sesion.spec.ts
│   ├── agenda/tablero.spec.ts
│   ├── ventas/registro.spec.ts
│   ├── ventas/caja.spec.ts
│   ├── barberos/gestion.spec.ts
│   ├── barberos/autogestion.spec.ts
│   ├── servicios/catalogo.spec.ts
│   ├── clientes/clientes.spec.ts
│   ├── reportes/reportes.spec.ts
│   ├── configuracion/configuracion.spec.ts
│   └── transversales/accesibilidad.spec.ts
├── paginas/                      # Page Objects: una clase por pantalla
│   ├── publico/LandingPagina.ts
│   ├── publico/AgendarPagina.ts
│   ├── panel/AgendaPagina.ts
│   └── …
├── fixtures/                     # fixtures de Playwright, componibles
│   ├── sistema.ts                # reinicio, reloj, ejecutar trabajos
│   ├── sesiones.ts               # páginas con sesión de administrador o de barbero
│   ├── whatsapp.ts               # leer mensajes simulados, firmar webhooks
│   └── correo.ts                 # leer correos de Mailpit
└── utilidades/
    ├── formato.ts                # $35.000, 9:00 a. m., fechas en español
    └── api.ts                    # cliente tipado para preparar datos sin pasar por la UI
```

Reglas:

- **Selectores accesibles**: `getByRole`, `getByLabel` y `getByText` con los textos visibles. `data-testid` solo cuando no hay un nombre accesible (como las celdas del tablero de la agenda). Así las pruebas también verifican la accesibilidad.
- **Preparar por API, verificar por UI**: los datos previos se crean con el cliente de `utilidades/api.ts`; la interfaz se usa solo para lo que la prueba quiere demostrar.
- **Sin esperas fijas**: nada de `waitForTimeout`; se usan las aserciones con reintento de Playwright o `expect.poll`.
- **Trazabilidad**: cada prueba lleva en su título el ID del escenario y los requisitos (por ejemplo, `E2E-RES-01 [CU-05, RF-RES-01]`) y la etiqueta `@humo` si pertenece a la suite rápida.

Ejemplo de fixture para leer el código de WhatsApp:

```ts
// e2e/fixtures/whatsapp.ts
import { test as base, expect } from '@playwright/test';

export const test = base.extend<{ ultimoCodigo: (celular: string) => Promise<string> }>({
  ultimoCodigo: async ({ request }, use) => {
    await use(async (celular) => {
      let codigo = '';
      await expect.poll(async () => {
        const r = await request.get(`/api/v1/_pruebas/mensajes?destinatario=${celular}`);
        const mensajes = await r.json();
        codigo = mensajes.find((m: any) => m.plantilla === 'codigo_verificacion')?.variables?.codigo ?? '';
        return codigo;
      }).toMatch(/^\d{4}$/);
      return codigo;
    });
  },
});
```

## 5. Ejecución en integración continua

| Momento | Suite | Proyectos | Tiempo objetivo |
|---|---|---|---|
| Cada cambio (pull request) | `@humo`: los 14 recorridos marcados con 🔥 | `movil-chrome` | < 5 min |
| Cada noche y antes de cada entrega | Completa | Los tres | < 15 min |

- **Ante un fallo**: traza, captura y video se adjuntan al reporte.
- **Reintentos**: uno en CI. Una prueba que pasa al reintentar se marca *inestable* y se corrige antes de la siguiente entrega.
- **Criterio de salida de una entrega**: todas las pruebas de prioridad Must en verde, cero inestables y cada requisito Must cubierto por al menos una prueba E2E o de integración.

## 6. Catálogo de escenarios

Formato: *Dado* (estado inicial) · *Cuando* (acción) · *Entonces* (resultado esperado). La columna P indica la prioridad: **M** (Must, bloquea la entrega) o **S** (Should). 🔥 marca la suite de humo.

### 6.1 Sitio público

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-PUB-01 🔥 | Dado el escenario base · Cuando abro la landing · Entonces veo los servicios activos con precio y duración (incluido el combo con su etiqueta) y no veo «Corte infantil». | M | RF-PUB-01, RF-SER-02 |
| E2E-PUB-02 | Dado que Andrés está de vacaciones y David retirado · Cuando veo «El equipo» · Entonces Andrés aparece con «De vacaciones hasta el 30 de noviembre» y sin botón de agendar, y David no aparece. | M | RF-PUB-02, RN-27 |
| E2E-PUB-03 | Dadas las franjas del escenario · Cuando veo «Ubicación» · Entonces el horario de lunes a viernes es 9:00 a. m. – 8:00 p. m., calculado desde los barberos. | M | RF-CFG-03, DA-10 |
| E2E-PUB-04 | Dado que la administradora cambia el precio del corte a $38.000 · Cuando recargo la landing tras vencer la caché · Entonces veo $38.000. | M | RF-PUB-04 |
| E2E-PUB-05 | Cuando pulso «Agendar con Santiago» o «Agendar» en un servicio · Entonces el agendamiento abre con esa opción preseleccionada. | S | RF-PUB-03 |
| E2E-PUB-06 | Cuando pulso el botón flotante de WhatsApp · Entonces el enlace apunta a `wa.me` con el número del negocio. | M | RF-PUB-07 |
| E2E-PUB-07 | Cuando abro las políticas desde el pie · Entonces veo la política de datos, la de cancelación y los términos. | M | RF-PUB-09 |

### 6.2 Agendamiento en línea

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-RES-01 🔥 | Dado un cliente nuevo en el celular · Cuando elige el combo, «Cualquier barbero», el jueves a las 3:30 p. m., verifica el código y escribe su nombre · Entonces ve la confirmación con el barbero asignado y el total, recibe por WhatsApp la confirmación con enlace y la cita aparece «Confirmada» en la agenda de la administradora, con origen en línea. | M | CU-05…08, RF-RES-01…12, RF-NOT-01, RF-SIS-02 |
| E2E-RES-02 🔥 | Dado el cliente habitual 310 555 9917 · Cuando escribe su celular · Entonces no ve ningún dato suyo hasta verificar el código; después ve «¡Hola de nuevo, Andrés!» y sus datos completados. | M | RF-RES-08, R-04 |
| E2E-RES-03 | Cuando elijo «Afeitado clásico» · Entonces los barberos que no lo ofrecen aparecen deshabilitados con «No realiza este servicio» y Andrés con su aviso de vacaciones. | M | RF-RES-03 |
| E2E-RES-04 | Dado el reloj a las 2:50 p. m. · Cuando elijo hoy · Entonces la primera hora ofrecida es a partir de las 4:50 p. m. (anticipación de 2 h), un día cerrado aparece sin cupos y un festivo usa los horarios de domingo. | M | RF-RES-05, RN-01…03 |
| E2E-RES-05 | Cuando envío el formulario con un celular que no empieza por 3, sin nombre y sin la autorización · Entonces veo un mensaje junto a cada campo y no se envía ningún código. | M | RF-RES-07 |
| E2E-RES-06 | Cuando escribo un código incorrecto cinco veces · Entonces el código queda invalidado y debo pedir uno nuevo; «Reenviar código» respeta el tiempo de espera. | M | RF-RES-09, RNF-SEG-05, RNF-SEG-06 |
| E2E-RES-07 🔥 | Dado que veo libre las 3:30 p. m. · Cuando otra persona la reserva antes que yo (preparada por API) y confirmo · Entonces veo «La hora ya no está disponible» con horas alternativas y puedo elegir otra sin pedir un código nuevo. | M | RF-RES-10, RNF-INT-01 |
| E2E-RES-08 🔥 | Dada una cita confirmada · Cuando abro el enlace del mensaje y la cancelo · Entonces queda «Cancelada», recibo el aviso por WhatsApp y esa hora vuelve a ofrecerse. | M | CU-10, RF-RES-14 |
| E2E-RES-09 | Dado el reloj a 2 h de la cita · Cuando la cancelo desde el enlace · Entonces queda cancelada y marcada como tardía, y no suma inasistencias al cliente. | M | RN-09, DA-17 |
| E2E-RES-10 | Cuando reprogramo desde el enlace al viernes · Entonces la cita cambia de hora y el recordatorio se programa para la hora nueva. | S | RF-RES-15 |
| E2E-RES-11 | Cuando desactivo los recordatorios desde el enlace · Entonces el worker no envía recordatorios para esa cita. | S | RF-RES-18, RF-NOT-02 |
| E2E-RES-12 | Cuando pulso «Agregar a mi calendario» · Entonces el enlace de Google Calendar trae el servicio, las horas de inicio y fin y la zona America/Bogota. | S | RF-RES-13 |
| E2E-RES-13 | En 375 px · Entonces la barra fija muestra el resumen y «Confirmar» durante todo el flujo, sin desplazamiento horizontal. | M | RF-RES-01, RNF-USA-01 |

### 6.3 Notificaciones

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-NOT-01 🔥 | Dada una cita mañana a las 3:00 p. m. · Cuando adelanto el reloj a hoy a las 3:00 p. m. y ejecuto los trabajos dos veces · Entonces se envía un solo recordatorio de 24 h. | M | RF-NOT-02 |
| E2E-NOT-02 | Dada una cita «Sin confirmar» · Cuando llega un webhook firmado con la respuesta «Confirmo» · Entonces la cita pasa a «Confirmada». | S | RF-NOT-03, RN-31 |
| E2E-NOT-03 | Cuando llega un webhook con firma inválida · Entonces se rechaza y nada cambia. | M | RNF-SEG-10 |
| E2E-NOT-04 | Dado que el WhatsApp simulado falla · Cuando un cliente reserva · Entonces la cita se crea igual, el mensaje se reintenta y, al agotar los intentos, la administradora ve un aviso. | M | RNF-DIS-03, RF-NOT-07 |

### 6.4 Acceso

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-AUT-01 🔥 | Cuando la administradora inicia sesión llega al dashboard; cuando Santiago inicia sesión llega a «Mis citas»; al cerrar sesión, ambos vuelven al ingreso y la sesión anterior deja de servir. | M | RF-AUT-01, RF-AUT-07 |
| E2E-AUT-02 | Cuando fallo la contraseña cinco veces · Entonces veo un mensaje genérico y la cuenta queda bloqueada temporalmente. | M | RNF-SEG-02, RNF-SEG-05 |
| E2E-AUT-03 | Dado un barbero nuevo con acceso al panel · Cuando abro la invitación en Mailpit y creo la contraseña · Entonces puedo entrar a su panel. | M | RF-AUT-02, RF-BAR-09 |
| E2E-AUT-04 | Cuando pido recuperar la contraseña y uso el enlace del correo · Entonces entro con la contraseña nueva y el enlace no se puede volver a usar. | M | RF-AUT-03, RNF-SEG-07 |
| E2E-AUT-05 🔥 | Dado Santiago con sesión · Cuando intento abrir `/admin` o pido por API una cita o un reporte de Mateo · Entonces la interfaz me devuelve a mi panel y la API responde 404. | M | RF-AUT-05, RNF-SEG-04 |
| E2E-AUT-06 | Dado Santiago con sesión abierta · Cuando la administradora lo suspende · Entonces su siguiente acción lo lleva al ingreso. | M | RF-AUT-08 |

### 6.5 Agenda del administrador

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-AGE-01 🔥 | Cuando abro la agenda de hoy · Entonces veo una columna por barbero con sus citas coloreadas por estado, los bloques fuera de turno y a Andrés como «Vacaciones»; en móvil veo la lista con filtro por barbero. | M | RF-AGE-01…03 |
| E2E-AGE-02 | Cuando creo una cita para un cliente nuevo que llamó · Entonces queda «Sin confirmar», con origen en el panel y la confirmación enviada. | M | RF-AGE-05, RN-31 |
| E2E-AGE-03 | Cuando reprogramo una cita a otra hora y otro barbero · Entonces el tablero se actualiza y el cliente recibe el aviso. | M | RF-AGE-06, RF-NOT-04 |
| E2E-AGE-04 | Cuando cancelo con el motivo «El barbero no puede atender» y aviso al cliente · Entonces la cita queda cancelada y el horario libre. | M | RF-AGE-07 |
| E2E-AGE-05 | Entonces «Marcar no asistió» está deshabilitado en las citas de mañana; cuando lo uso en una de hoy, el cliente suma una inasistencia en su ficha. | M | RF-AGE-08 |
| E2E-AGE-06 | Cuando pulso «Iniciar» en una cita confirmada · Entonces pasa a «En curso». | S | RF-AGE-10 |

### 6.6 Ventas y caja

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-VEN-01 🔥 | Dada la cita en curso de Pablo Vélez · Cuando la atiendo desde la agenda, agrego cejas, aplico 10 % con motivo, propina libre de $3.500 y pago por Nequi · Entonces el total es correcto, el ticket es el siguiente consecutivo, la cita queda «Atendida» y la caja del día suma la venta. | M | RF-VEN-01…10, RF-AGE-09 |
| E2E-VEN-02 | Cuando registro una venta a un cliente nuevo · Entonces el cliente se crea; cuando registro una a un cliente ocasional, ningún historial cambia. | M | RF-VEN-03, RF-CLI-01 |
| E2E-VEN-03 | Cuando cambio el barbero a uno que no ofrece un servicio ya agregado · Entonces ese servicio se quita. | M | RF-VEN-04 |
| E2E-VEN-04 🔥 | Dado Santiago con sesión · Cuando registra una venta · Entonces el barbero está fijo, solo ve los descuentos permitidos, no puede anular y «Lo que llevas hoy» suma comisión y propina. | M | RF-PBA-03…05 |
| E2E-VEN-05 | Cuando la administradora anula una venta con motivo · Entonces sale de la caja y de los reportes y queda en la auditoría. | M | RF-VEN-12, RF-SIS-07 |
| E2E-VEN-06 | Cuando hago el cierre de caja · Entonces lo esperado por método incluye las propinas, veo la diferencia con lo contado y un segundo cierre del mismo día se rechaza. | S | RF-VEN-11, DA-09 |

### 6.7 Barberos

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-BAR-01 🔥 | Cuando registro un barbero con datos esenciales, horario «igual toda la semana» y dos servicios · Entonces aparece en la landing y en el agendamiento para esos servicios. | M | RF-BAR-04, RF-BAR-07…09 |
| E2E-BAR-02 | Dado que Mateo tiene citas el viernes · Cuando registro sus vacaciones · Entonces veo esas citas antes de guardar, las reasigno a Julián y los clientes reciben el aviso. | M | RF-BAR-11, RF-BAR-13, RF-BAR-14 |
| E2E-BAR-03 | Dado Andrés de vacaciones hasta el 30 · Cuando el reloj pasa al 1 de diciembre · Entonces aparece «Activo» y vuelve a recibir citas, sin que nadie lo cambie. | M | RF-SIS-03, RN-24 |
| E2E-BAR-04 | Cuando retiro a un barbero · Entonces desaparece de la landing y del agendamiento y su historial sigue en los reportes. | M | RF-BAR-15 |
| E2E-BAR-05 | Dado Santiago con citas el sábado · Cuando acorta su horario del sábado · Entonces ve las citas afectadas antes de guardar y la administradora recibe un aviso. | S | RF-PBA-09, DA-08 |
| E2E-BAR-06 🔥 | Cuando Santiago bloquea mañana de 2:00 a 4:00 p. m. · Entonces esas horas dejan de ofrecerse en el agendamiento. | M | RF-PBA-10 |

### 6.8 Servicios

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-SER-01 | Cuando creo el combo «Corte + cejas» con precio propio · Entonces se puede agendar en línea como un solo servicio. | M | RF-SER-01, DA-02 |
| E2E-SER-02 | Cuando desactivo «Diseño de cejas» · Entonces desaparece de la landing y del agendamiento y sus ventas pasadas siguen en los reportes. | M | RF-SER-02 |
| E2E-SER-03 | Dada una cita futura del corte a $35.000 · Cuando cambio el precio a $38.000 · Entonces esa cita conserva $35.000 y las nuevas usan $38.000. | M | RF-SER-03 |

### 6.9 Clientes

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-CLI-01 | Cuando busco «andres felipe» (sin tilde) o «555 99» · Entonces encuentro a Andrés Felipe Mora. | M | RF-CLI-02 |
| E2E-CLI-02 | Cuando se registra una venta a un cliente · Entonces su ficha muestra una visita más y el total gastado actualizado. | M | RF-CLI-03 |
| E2E-CLI-03 | Cuando cambio el umbral de «Frecuente» · Entonces cambia el segmento que se muestra para los clientes afectados. | S | RF-CLI-04, RF-CFG-08 |
| E2E-CLI-04 | Cuando anonimizo un cliente a petición suya · Entonces desaparece de las búsquedas y los reportes conservan sus cifras. | M | RF-CLI-07, RNF-LEG-03 |

### 6.10 Reportes y dashboard

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-REP-01 | Dado el escenario base · Cuando abro el dashboard · Entonces las ventas, los clientes y las citas por estado coinciden con lo sembrado. | M | RF-DAS-01, RF-DAS-03 |
| E2E-REP-02 🔥 | Cuando genero la liquidación de Santiago del mes · Entonces la comisión es venta neta × su porcentaje y las propinas incluyen las pagadas en efectivo y por Nequi. | M | RF-REP-03, RN-15, DA-09 |
| E2E-REP-03 | Cuando descargo el resumen en PDF · Entonces recibo un PDF válido con el periodo en el encabezado. | M | RF-REP-06 |
| E2E-REP-04 | Dado Santiago con sesión · Entonces en «Mi reporte» solo puede ver su liquidación. | M | RF-PBA-06 |

### 6.11 Configuración

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-CFG-01 | Cuando marco el 24 de diciembre como cierre temprano a las 2:00 p. m. · Entonces ese día no se ofrecen horas después de las 2:00 p. m. | M | RF-CFG-04 |
| E2E-CFG-02 | Cuando cambio la anticipación mínima a 30 minutos · Entonces hoy se ofrecen horas desde las 3:30 p. m. | S | RF-CFG-02 |
| E2E-CFG-03 | Cuando desactivo «Daviplata» · Entonces deja de aparecer en el formulario de venta. | S | RF-CFG-07 |

### 6.12 Transversales

| ID | Escenario | P | Requisitos |
|---|---|---|---|
| E2E-TRV-01 | En la landing, el agendamiento, el ingreso, la agenda y la venta · Entonces axe no reporta violaciones serias ni críticas. | M | RNF-ACC-01, RNF-ACC-02 |
| E2E-TRV-02 | En 320, 375, 768 y 1280 px · Entonces no hay desplazamiento horizontal, y en móvil los paneles usan la barra inferior. | M | RNF-USA-01, RNF-USA-07 |
| E2E-TRV-03 | Cuando completo una reserva usando solo el teclado · Entonces llego a la confirmación con el foco siempre visible. | M | RNF-ACC-02 |
| E2E-TRV-04 | Entonces las páginas del panel y de gestión de citas incluyen `noindex`. | M | RNF-SEO-02 |

## 7. Cobertura

| Área | Escenarios | Must cubiertos |
|---|---|---|
| Sitio público | 7 | RF-PUB-01, 02, 04, 07, 09; RF-CFG-03 |
| Agendamiento | 13 | RF-RES-01, 02, 03, 05, 07, 09, 10, 11, 12, 14 |
| Notificaciones | 4 | RF-NOT-01, 02, 04 |
| Acceso | 6 | RF-AUT-01, 02, 03, 05, 07, 08 |
| Agenda | 6 | RF-AGE-01…09 |
| Ventas | 6 | RF-VEN-01…10, 12 |
| Barberos | 6 | RF-BAR-04, 07, 08, 09, 11, 13, 15; RF-PBA-10 |
| Servicios | 3 | RF-SER-01, 02, 03 |
| Clientes | 4 | RF-CLI-01, 02, 03, 07 |
| Reportes | 4 | RF-REP-01, 02, 03, 06; RF-DAS-01, 03 |
| Configuración | 3 | RF-CFG-04 |
| Transversales | 4 | RNF de accesibilidad, responsive y SEO |
| **Total** | **66** | |

Los requisitos Must que no tienen escenario E2E propio (por ejemplo, RF-SIS-07 auditoría fuera de las anulaciones, o los RNF de integridad) se verifican con pruebas de integración del backend, como las 27 pruebas del esquema de base de datos.
