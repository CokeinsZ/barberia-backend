# Módulo `clientes`

Clientes, autorizaciones de tratamiento de datos, segmentos y proyección de métricas
(RF-CLI-01 a 04, RF-RES-11).

| | |
|---|---|
| Fase del plan | 3 ✅ (registro y búsqueda) · 7 (habeas data, anonimización, segmentos configurables) |
| Esquema de base de datos | `clientes` (ver `migraciones/`); lectura por la vista `v_cliente_resumen` |
| Fachada | `api::ClientesAgenda<C>`: `por_celular`, `por_ids`, `registrar_en_linea`, `registrar_desde_panel`, `cambiar_recordatorios` (la usan `agenda` y, a través de `app`, `verificacion`) |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Notas |
|---|---|---|---|
| GET | `/clientes/buscar?q=` | Personal | Búsqueda rápida por nombre o celular (2+ caracteres, máximo 8, sin tildes) |
| GET | `/clientes?q=&segmento=&pagina=&por_pagina=` | Administrador | Listado con visitas, gasto, inasistencias y segmento |
| GET | `/clientes/{id}` | Administrador | Ficha con autorizaciones de datos |
| POST | `/clientes` | Personal | `409 celular_en_uso` si el celular ya existe |
| PUT | `/clientes/{id}` | Administrador | Datos, preferencias y recordatorios |

## Reglas

- El celular es la identidad del cliente (único, formato colombiano `3XXXXXXXXX`).
- **En línea** (al reservar): se crea o actualiza por celular (`ON CONFLICT`) y se guarda la
  autorización con la versión de la política, la IP y el agente (RF-RES-11, Ley 1581).
- **Desde el panel**: el cliente que llamó se registra sin autorización en línea.
- Segmentos: `sin_visitas`, `nuevo` (1 visita), `recurrente`, `frecuente` e `inactivo` (sin
  volver en mucho tiempo); se calculan en el dominio a partir de la proyección de métricas.

## Eventos que publica (crate `contratos`)

- Ninguno.

## Eventos que consume

- `InasistenciaMarcada` (agenda): suma una inasistencia en `metricas` (dentro de la misma
  transacción que marca la cita).
- `VentaRegistrada`, `VentaAnulada` (ventas, fase 5): visitas y gasto.

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
