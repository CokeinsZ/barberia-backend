# Módulo `barberos`

El equipo (RF-BAR-01 a 19, RF-PBA-07 a 10): datos esenciales y privados, cuenta del panel,
horario, servicios, ausencias, bloqueos, retiro, visibilidad, orden, foto y documentos.

| | |
|---|---|
| Fase del plan | 2 ✅ |
| Esquema de base de datos | `barberos` (ver `migraciones/`) |
| Depende de | `identidad` (`CuentasPersonal`), `catalogo` (`CatalogoServicios`), `archivos` (`Archivos`), `negocio` (`ParametrosNegocio`), `auditoria` |
| Implementa puertos de | `identidad` (`PerfilesBarberos`) y `catalogo` (`OferentesServicio`) |
| Fachadas que ofrece | `EquipoAgenda` (franjas, ausencias, bloqueos y retiro, para `agenda`) y `EquipoVentas` (estado en la fecha, servicios y comisión vigente, para `ventas`) |
| Publica (crate `contratos`) | `HorarioCambiado`, `BloqueoCreado`, `AusenciaRegistrada` (la agenda avisa a la administradora si dejan citas por fuera) y `AccesoRevocado` |

## Endpoints (`/api/v1`)

| Método | Ruta | Acceso |
|---|---|---|
| GET | `/yo` | Barbero: su ficha en solo lectura (RF-PBA-07) |
| GET / POST | `/barberos?q=&estado=` | Administrador: listado con indicadores / registro con cuenta, horario, servicios y visibilidad |
| PUT | `/barberos/orden` | Administrador: `{ barbero_ids }` con todos los vigentes |
| GET / PUT | `/barberos/{id}` | Administrador: ficha / datos esenciales (cambiar el correo cambia el usuario del panel) |
| PUT | `/barberos/{id}/visibilidad` | Administrador: landing, citas en línea y acceso al panel |
| POST | `/barberos/{id}/retiro` | Administrador: `{ fecha, motivo }` |
| GET / PUT | `/barberos/{id}/horario` | Administrador o el propio barbero |
| GET / PUT | `/barberos/{id}/servicios` | Administrador o el propio barbero |
| GET / PUT | `/barberos/{id}/datos-privados` | Administrador (grupo sanguíneo y número de cuenta cifrados) |
| POST | `/barberos/{id}/foto` | Administrador: multipart `foto`, imagen vertical de hasta 5 MB |
| GET / POST | `/barberos/{id}/documentos` | Administrador: expediente con faltantes / multipart `tipo`, `periodo` (PILA, `AAAA-MM`) y `archivo` |
| GET | `/barberos/{id}/documentos/{doc}/descarga` | Administrador: redirige a una URL firmada de 5 minutos |
| DELETE | `/barberos/{id}/documentos/{doc}` | Administrador |
| GET / POST | `/barberos/{id}/ausencias` | Administrador (RN-23) |
| POST | `/barberos/{id}/ausencias/{aid}/anular` | Administrador |
| POST | `/barberos/{id}/ausencias/{aid}/soporte` | Administrador: multipart `soporte` |
| GET / POST | `/barberos/{id}/bloqueos?desde=` | Administrador o el propio barbero |
| DELETE | `/barberos/{id}/bloqueos/{bid}` | Administrador o el propio barbero |

Un barbero que pide algo de otro recibe **404**, para no revelar que existe.

## Reglas

- **Estado por fecha** (RN-24): no se guarda; se calcula con `barberos.estados_en(fecha)`:
  retirado > suspendido > incapacidad o licencia > vacaciones > activo. Una ausencia futura no
  cambia el estado de hoy y al terminar vuelve solo a «Activo» (E2E-BAR-03).
- **Cruces**: ausencias, bloqueos y franjas los impide la base con restricciones de exclusión;
  el módulo responde 409 (`ausencia_solapada`, `bloqueo_solapado`). Horario: hasta 3 franjas por
  día, sin cruces y con la hora final posterior a la inicial.
- **Cuenta**: todo barbero tiene cuenta; «Acceso al panel» la activa (y envía la invitación si no
  se ha activado) o la desactiva (y cierra sus sesiones). Retirarlo hoy quita el acceso; un
  retiro programado lo quita en su fecha, porque `identidad` revisa el estado del día.
- **Datos sensibles** (RNF-LEG-04): AES-256-GCM con `DIAMANTE_CLAVE_CIFRADO`; la auditoría
  guarda qué campos cambiaron, nunca sus valores.
- **Archivos**: foto pública y vertical; documentos y soportes privados con URL firmadas.

## Pruebas

Las reglas viven sobre todo en la base (cruces, estado por fecha), así que los casos de uso se
prueban de punta a punta: `crates/app/src/pruebas_postgres.rs` (registro, panel del barbero,
horario, bloqueos, ausencias, foto, documentos, datos cifrados, orden y retiro) y
`tests/perfiles_postgres.rs`. El dominio tiene pruebas unitarias en `src/dominio/`.

Pendiente para la fase 3 (con `agenda`): mostrar las citas afectadas antes de guardar una
ausencia o un cambio de horario y avisar al administrador (RF-BAR-13, RF-BAR-14, RF-PBA-09).
