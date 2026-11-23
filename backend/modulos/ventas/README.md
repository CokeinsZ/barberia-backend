# Módulo `ventas`

Ventas, pagos, descuentos, caja del día y cierres de caja (RF-VEN-01 a 13, RF-PBA-03 a 05,
RF-CFG-07).

| | |
|---|---|
| Fase del plan | 5 ✅ |
| Esquema de base de datos | `ventas` (ver `migraciones/`); vista publicada `v_venta` para reportes |
| Fachadas que usa | `agenda` (`CitasParaVentas`: la cita que se cobra, bloqueada), `barberos` (`EquipoVentas`: estado en la fecha, servicios y comisión), `catalogo` (precios vigentes), `clientes` (cliente nuevo) y `auditoria` |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/ventas/catalogos` | Personal | Métodos activos, descuentos (el barbero solo los permitidos a barberos), motivos y propinas rápidas |
| POST | `/ventas` | Personal | Ver «Registrar» abajo. `201` con la venta |
| GET | `/ventas?fecha=&desde=&hasta=&barbero_id=&cliente_id=&incluir_anuladas=&pagina=` | Personal | Sin fechas ni cliente: las de hoy. El barbero solo ve las suyas |
| GET | `/ventas/{id}` | Personal | Lo ajeno no existe para el barbero (404) |
| POST | `/ventas/{id}/anulacion` | Administrador | `{ motivo }` (5 a 300 caracteres). Conserva el número de ticket; auditada |
| GET | `/ventas/caja?fecha=&barbero_id=` | Administrador | Totales sin anuladas, desglose por método (propinas incluidas) y ventas del día |
| GET | `/ventas/resumen-barbero?fecha=&barbero_id=` | Barbero (el suyo); administrador con `barbero_id` | «Lo que llevas hoy»: comisión + propinas |
| GET / POST | `/ventas/cierres` (`?fecha=` al consultar) | Administrador | `{ fecha, contado: [{ metodo_pago_id, monto }], observacion }`; uno por día (`409 cierre_existente`) |
| GET | `/ventas/metodos-pago` | Administrador | Todos, activos o no |
| PATCH | `/ventas/metodos-pago/{id}` | Administrador | `{ activo }`; siempre queda al menos uno activo; auditado |

## Registrar una venta

```json
{ "cita_id": 1842, "cliente": { "tipo": "registrado", "id": 57 }, "barbero_id": 1,
  "servicio_ids": [3, 5], "descuento": { "descuento_id": 1, "motivo_descuento_id": 1 },
  "propina": 5000, "pagos": [{ "metodo_pago_id": 2, "monto": 65300 }] }
```

- `cliente`: `{ "tipo": "registrado", "id" }`, `{ "tipo": "nuevo", "nombre", "celular", "correo" }`
  (se crea; `409 celular_en_uso` si existe) u `{ "tipo": "ocasional" }` (no suma a ningún historial).
  Con `cita_id`, el cliente es el de la cita.
- **Barbero**: activo en la fecha y que ofrezca todos los servicios (RN-05). Si registra un
  barbero, queda a su nombre aunque pida otro (RF-PBA-04).
- **Cita**: de hoy o anterior, por atender; una cita atendida responde `409 cita_ya_cobrada`.
  La cita queda «Atendida» en la misma transacción (RF-VEN-09). Una cita nunca tiene dos ventas
  vigentes (índice único parcial `venta_cita_vigente_uk`, solo entre las registradas): si su
  venta se anula, se puede volver a cobrar.
- **Totales** (los calcula el servidor): subtotal con el precio vigente, descuento con motivo
  (nunca mayor que el subtotal; el barbero solo los permitidos), venta neta, propina libre (100 %
  del barbero, no paga comisión), total y comisión con el porcentaje vigente del barbero, que se
  copia en la venta (RN-12 a 15, RNF-INT-05).
- **Pagos**: métodos activos y distintos; la suma debe ser exactamente el total (admite dos
  métodos, RF-VEN-13). La base vuelve a verificar el cuadre al confirmar (triggers diferidos).
- **Montos**: ningún valor en pesos pasa de un billón (`Dinero::MAXIMO`): la propina (tope de
  cordura de $1.000.000), los pagos y lo contado en el cierre se validan antes de convertirlos,
  así que un valor fuera de rango es un `422` en su campo y nunca un 0 en silencio. Las sumas se
  hacen comprobadas: un monto absurdo no desborda la suma de los pagos ni la diferencia del cierre.
- **Ticket**: consecutivo sin huecos con el contador bloqueado en la transacción (RN-19).

## Eventos que publica (crate `contratos`)

- `VentaRegistrada`: `agenda` deja la cita «Atendida»; `clientes` suma visita, gasto, última
  visita y barbero habitual.
- `VentaAnulada`: `clientes` resta la visita y el gasto y recalcula la última visita con las
  ventas vigentes (las calcula este módulo y van en el evento); `agenda` devuelve la cita a
  «Confirmada» (fase 8): se cobra de nuevo con la venta correcta, o se marca la inasistencia si
  se cobró a la cita equivocada. Así «Atendida» siempre tiene una venta vigente detrás. (Antes la
  cita seguía «Atendida» sin venta y no había forma de corregirla.)

## Eventos que consume

- Ninguno.

## Estructura

```text
ventas/src/
├── api.rs                 # DTO y ErrorVentas
├── modulo.rs              # DependenciasVentas y rutas()
├── dominio/               # totales, descuentos, comisión y cuadre de pagos (con pruebas)
├── aplicacion/            # ventas (registrar, listar, detalle, anular) y caja (caja, resumen, cierres, métodos)
├── infraestructura/       # RepositorioVentasPg
└── http/                  # rutas del panel
```
