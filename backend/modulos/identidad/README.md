# Módulo `identidad`

Cuentas del personal (administradores y barberos), sesiones, invitaciones, recuperación de
contraseña y gestión de administradores. Los clientes no tienen cuenta.

| | |
|---|---|
| Fase del plan | 1 ✅ |
| Esquema de base de datos | `identidad` (ver `migraciones/`) |
| Requisitos | RF-AUT-01 a 05, RF-AUT-07, RF-AUT-08 (parcial, ver abajo), RF-CFG-06, RNF-SEG-02, 05 y 07 |
| Depende de | `auditoria` (`Auditor`), `notificaciones` (`BuzonSalida`) |
| Puerto que implementa otro módulo | `api::PerfilesBarberos` (lo implementa `barberos`) |
| Fachada para otros módulos | `api::CuentasPersonal<C>`: `barberos` crea la cuenta del barbero, le cambia el correo y le da o quita el acceso (con invitación y cierre de sesiones), dentro de su transacción |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso | Respuesta |
|---|---|---|---|
| POST | `/auth/sesion` | Público, 10 por IP cada 15 min | Cookie `__Host-sesion` y `{ usuario: { id, rol, nombre, correo }, panel }` |
| GET | `/auth/sesion` | Personal | Igual que el inicio de sesión |
| DELETE | `/auth/sesion` | Cualquiera | 204 y cookie vencida |
| POST | `/auth/activacion` | Público | `{ token, contrasena }` → 204; 410 `enlace_vencido` |
| POST | `/auth/recuperacion` | Público, 3 por correo y 10 por IP cada hora | 202 exista o no la cuenta |
| POST | `/auth/recuperacion/confirmar` | Público | `{ token, contrasena }` → 204; cierra todas las sesiones |
| PUT | `/auth/contrasena` | Personal | `{ actual, nueva }` → 204; cierra las demás sesiones |
| GET | `/administradores` | Administrador | Lista con `pendiente_activacion` y `ultimo_acceso_en` |
| POST | `/administradores` | Administrador | `{ correo, nombre }` → 201 `{ id }`; el enlace viaja solo por correo |
| PATCH | `/administradores/{id}` | Administrador | `{ activo }` → 204 |

Errores propios: `credenciales_invalidas` (401, mensaje genérico), `acceso_suspendido` (403),
`enlace_vencido` (410), `correo_en_uso`, `ultimo_administrador` y `no_puede_desactivarse` (409).

Línea de comandos (actor `sistema`, auditado):

- `diamante crear-administrador <correo> <nombre>` invita al primer administrador y muestra el
  enlace de activación.
- `diamante enlace-acceso <correo>` muestra un enlace nuevo para una cuenta con acceso: de
  activación si no la ha activado, de recuperación si ya lo hizo. No envía correo; sirve
  cuando el correo no llega y para las pruebas contra preproducción.

## Reglas de seguridad

- **Tokens**: 256 bits aleatorios en base64url. La base guarda solo su SHA-256: un volcado de la
  base no permite usar sesiones ni enlaces.
- **Sesiones** en el servidor, deslizantes: 12 horas sin uso, o 30 días con «Recordarme». El uso
  se registra como máximo cada 5 minutos. Cada petición revalida la cuenta: una sesión de una
  cuenta desactivada o de un barbero retirado o suspendido hoy deja de servir de inmediato.
- **Contraseñas**: Argon2id; de 12 a 128 caracteres. Con un correo inexistente también se
  verifica contra un hash ficticio, para que el tiempo de respuesta no revele si la cuenta existe.
- **Bloqueo**: 5 fallos seguidos bloquean la cuenta 15 minutos (auditado como `bloquear_cuenta`).
  La fila de la cuenta se bloquea (`FOR UPDATE`) para contar bien los intentos simultáneos.
- **Enlaces** de un solo uso: activación 7 días, recuperación 1 hora; emitir uno nuevo invalida
  los anteriores. El token va en el fragmento (`/acceso/activar#token=…`): el navegador no lo
  envía al servidor, así no queda en logs ni en `Referer`.
- **Último administrador**: las filas de los administradores activos se bloquean antes de
  desactivar, para que dos desactivaciones simultáneas no dejen la barbería sin ninguno.

## Auditoría

`bloquear_cuenta`, `activar_cuenta`, `restablecer_contrasena`, `cambiar_contrasena`,
`invitar_administrador`, `activar_administrador` y `desactivar_administrador`, en la misma
transacción que el cambio.

## Pendiente

- RF-AUT-08 ya se cumple: `barberos` quita el acceso por la fachada `CuentasPersonal` (en la
  misma transacción) y, además, cada petición revalida el estado del día del barbero.
- Limpieza de sesiones y enlaces vencidos: tarea de mantenimiento del módulo `trabajos` (fase 4).

## Estructura

```text
identidad/src/
├── api.rs                 # fachada: DTO, ErrorIdentidad, PerfilesBarberos
├── modulo.rs              # DependenciasIdentidad → rutas, validador de sesiones, invitación (CLI)
├── dominio/               # Usuario (bloqueo), ContrasenaNueva, DuracionSesion, PropositoToken
├── aplicacion/
│   ├── puertos.rs         # repositorios genéricos en C, Credenciales y Puertos<C>
│   ├── comandos/          # iniciar y cerrar sesión, activar, recuperar, cambiar contraseña, administradores
│   ├── consultas/         # validar y consultar sesión, listar administradores
│   └── pruebas.rs         # 13 escenarios con dobles en memoria (C = ())
├── infraestructura/
│   ├── postgres/          # usuarios, sesiones y tokens
│   └── seguridad/         # Argon2id
└── http/                  # rutas, manejadores, límites de intentos y ValidadorSesion
tests/api_postgres.rs      # HTTP → PostgreSQL real → Argon2 real → correos en la cola
```
