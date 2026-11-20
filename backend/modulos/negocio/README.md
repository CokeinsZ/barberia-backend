# Módulo `negocio` (módulo de referencia)

Datos de la barbería (nombre, NIT, contacto, comisión por defecto) y días especiales
(festivos, cierres y cierres tempranos). Es la **plantilla** que siguen todos los módulos.

| | |
|---|---|
| Esquema de base de datos | `negocio` (ver `migraciones/`) |
| Requisitos | RF-CFG-01, RF-CFG-04 |
| Endpoints | `GET/PUT /api/v1/negocio`, `GET/POST /api/v1/negocio/dias-especiales`, `DELETE /api/v1/negocio/dias-especiales/{id}` |
| Fachada para otros módulos | `api::CalendarioNegocio` (la usará `agenda` para la disponibilidad) |
| Pendiente | Auditoría de los cambios (cuando exista el módulo `auditoria`, fase 1) y precarga de festivos (RF-CFG-05) |

## Estructura

```text
negocio/
├── Cargo.toml
├── README.md
├── migraciones/                         # SQL del esquema `negocio`, solo de este módulo
├── src/
│   ├── lib.rs                           # publica solo `api`, `ModuloNegocio` y `MIGRACIONES`
│   ├── api.rs                           # fachada: DTO, error y traits para otros módulos
│   ├── modulo.rs                        # construcción: recibe dependencias, entrega rutas y fachada
│   ├── dominio/                         # reglas puras: sin SQL, sin HTTP, sin otros módulos
│   │   ├── errores.rs                   # ErrorDominio y acumulador de errores de campo
│   │   └── entidades/
│   │       ├── datos_negocio.rs         # se construye solo si es válido (DatosNegocio::nuevo)
│   │       └── dia_especial.rs          # tipos de día y regla de fechas pasadas
│   ├── aplicacion/
│   │   ├── mod.rs                       # CasosDeUso: todos los casos de uso ya conectados
│   │   ├── errores.rs                   # ErrorNegocio → categoría y código estable
│   │   ├── politicas.rs                 # quién puede ejecutar cada caso de uso
│   │   ├── dto.rs                       # datos de salida
│   │   ├── puertos/                     # traits pequeños que la aplicación necesita
│   │   ├── consultas/                   # un archivo por consulta
│   │   ├── comandos/                    # un archivo por comando
│   │   └── pruebas.rs                   # casos de uso con repositorios en memoria
│   ├── infraestructura/postgres/        # implementación de los puertos con SQLx
│   └── http/                            # rutas, manejadores, solicitudes y respuestas
└── tests/
    └── api_postgres.rs                  # punta a punta: HTTP → caso de uso → PostgreSQL real
```

## Reglas que este módulo demuestra

1. **Dependencias en una sola dirección**: `http → aplicacion → dominio`; `infraestructura`
   implementa los puertos de `aplicacion`. El dominio no importa `sqlx` ni `axum`.
2. **Un caso de uso por archivo**, con su política: `actualizar_datos_negocio.rs` llama a
   `politicas::solo_administrador` antes de hacer nada.
3. **Entidades siempre válidas**: `DatosNegocio::nuevo` valida y acumula todos los errores de
   campo para mostrarlos juntos en la interfaz.
4. **Errores tipados**: `ErrorNegocio` implementa `ErrorDeAplicacion` (categoría + código), y
   `plataforma` lo convierte en *Problem Details* sin que el módulo conozca HTTP.
5. **La base de datos es la última defensa**: la restricción única de la fecha llega como
   `ErrorPersistencia::Duplicado` y el caso de uso la traduce a `FechaOcupada` (409).
6. **Seguro por defecto**: todos los manejadores piden `ActorAutenticado`; sin sesión, 401.
7. **Tiempo inyectado**: «hoy» sale del `Reloj`, así las pruebas fijan la fecha.
8. **Fachada mínima**: otros módulos solo ven `api.rs`; todo lo demás es `pub(crate)`.

## Cómo agregar un caso de uso

1. Si hace falta, agregar la regla en `dominio/` con sus pruebas unitarias.
2. Crear `aplicacion/comandos/<accion>.rs` (o `consultas/`) con su política y registrarlo en
   `aplicacion/mod.rs` (`CasosDeUso`).
3. Si necesita algo nuevo del exterior, agregar un método a un puerto (o un puerto nuevo) e
   implementarlo en `infraestructura/postgres/` y en los dobles de `aplicacion/pruebas.rs`.
4. Exponerlo en `http/rutas.rs` y `http/manejadores.rs`.
5. Cubrirlo en `aplicacion/pruebas.rs` y, si toca la base de datos, en `tests/api_postgres.rs`.

## Pruebas

```bash
cargo test -p negocio                                            # unitarias y de casos de uso
PRUEBAS_DATABASE_URL=postgres://… cargo test -p negocio --test api_postgres   # integración
```
