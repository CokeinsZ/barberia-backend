# Módulo `clientes`

Clientes, autorizaciones de tratamiento de datos, segmentos configurables y proyección de
métricas (RF-CLI-01 a 04, RF-CFG-08, RF-RES-11).

| | |
|---|---|
| Fase del plan | 3 ✅ (registro y búsqueda) · 5 ✅ (métricas por ventas, anonimización y solicitudes del titular) · 8 ✅ (segmentos configurables e indicadores de la base) |
| Esquema de base de datos | `clientes` (ver `migraciones/`); lectura por la vista `v_cliente_resumen` |
| Fachada | `api::ClientesAgenda<C>`: `por_celular`, `por_ids`, `registrar_en_linea`, `registrar_desde_panel`, `cambiar_recordatorios` (la usan `agenda` y, a través de `app`, `verificacion`) |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/clientes/buscar?q=` | Personal | Búsqueda rápida por nombre o celular (2+ caracteres, máximo 8, sin tildes) |
| GET | `/clientes?q=&segmento=&pagina=&por_pagina=` | Administrador | Listado con visitas, gasto, inasistencias y segmento; en `meta`, los indicadores de toda la base (registrados y cuántos hay en cada segmento), sin importar los filtros |
| GET / PUT | `/clientes/configuracion-segmentos` | Administrador | Umbrales: `max_visitas_nuevo` (1–99), `max_visitas_recurrente` (mayor que el anterior, hasta 100) y `dias_inactividad` (15–365). Cada cambio queda en la auditoría (`cambiar_segmentos`, con el antes y el después) |
| GET | `/clientes/{id}` | Administrador | Ficha con autorizaciones de datos |
| POST | `/clientes` | Personal | `409 celular_en_uso` si el celular ya existe |
| PUT | `/clientes/{id}` | Administrador | Datos, preferencias y recordatorios |
| POST | `/clientes/{id}/anonimizacion` | Administrador | `{ motivo }` → `204`. Borra nombre, celular, correo, cumpleaños y preferencias; revoca la autorización y conserva las cifras (RF-CLI-07). `409` si tiene citas por atender |
| GET | `/clientes/solicitudes-titular?abiertas=true&pagina=` | Administrador | Solicitudes de habeas data con su plazo legal (`responder_antes_de`) |
| POST | `/clientes/solicitudes-titular` | Administrador | `{ cliente_id, tipo: consulta \| actualizacion \| supresion, descripcion }` |
| PATCH | `/clientes/solicitudes-titular/{id}` | Administrador | `{ respuesta }`; una sola vez (RNF-LEG-03) |

## Reglas

- El celular es la identidad del cliente (único, formato colombiano `3XXXXXXXXX`).
- **En línea** (al reservar): se crea o actualiza por celular (`ON CONFLICT`) y se guarda la
  autorización con la versión de la política, la IP y el agente (RF-RES-11, Ley 1581).
- **Desde el panel**: el cliente que llamó se registra sin autorización en línea.
- Segmentos (RN-20, DA-07): `sin_visitas`; `inactivo` («sin volver») si la última visita fue hace
  **más** de `dias_inactividad` días (prevalece); si no, `nuevo` hasta `max_visitas_nuevo`
  visitas, `recurrente` hasta `max_visitas_recurrente` y `frecuente` de ahí en adelante. Por
  defecto 2, 5 y 60. Se calculan al consultar con los umbrales vigentes (RF-SIS-06): un cambio se
  ve de inmediato en el listado, la ficha, la búsqueda rápida y los reportes. Los conteos de
  `meta` se hacen en SQL con la misma regla (una prueba compara ambos, incluido el borde exacto).
- La ficha trae el ticket promedio (total gastado ÷ visitas, RN-21); las últimas visitas salen
  de `GET /ventas?cliente_id=`.
- La búsqueda ignora tildes y mayúsculas en el nombre y compara los dígitos del celular aunque se
  escriban con espacios («555 99», E2E-CLI-01).
- Plazos de las solicitudes del titular (Ley 1581): 10 días hábiles las consultas y 15 los
  reclamos (actualización o supresión). Es una alerta: no descuenta festivos.
- Puerto `api::AgendaDelCliente`: lo implementa `app` con la agenda (citas por atender).

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- `InasistenciaMarcada` (agenda): suma una inasistencia en `metricas` (dentro de la misma
  transacción que marca la cita).
- `VentaRegistrada` (ventas): suma la visita, el gasto (venta neta), la última visita y la visita
  con ese barbero (barbero habitual). Las ventas a «cliente ocasional» no suman.
- `VentaAnulada` (ventas): resta la visita y el gasto y toma la última visita que trae el evento.

## Estructura

```text
clientes/src/
├── api.rs                 # ClientesAgenda<C>, ClienteConocido, DatosReservaEnLinea, NuevoCliente
├── modulo.rs              # con_postgres, suscribir(bus), rutas(), clientes()
├── dominio/               # validación de datos y segmento
├── aplicacion/            # casos (buscar, listar, ficha, crear, editar) y fachada
├── infraestructura/       # RepositorioClientesPg y la proyección de inasistencias
└── http/                  # rutas del panel
```
