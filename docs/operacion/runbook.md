# Runbook de operación

Cómo desplegar, actualizar, respaldar, restaurar y vigilar Diamante Barbería en producción.
Está escrito para quien administre el servidor (VPS con Docker). Todos los comandos se
ejecutan en la carpeta del proyecto.

## 1. Servicios en producción

`docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d` levanta:

| Servicio | Qué hace | Puertos | Volúmenes |
|---|---|---|---|
| `proxy` | Caddy: HTTPS automático (Let's Encrypt), cabeceras de seguridad, `/api/*` a la API, fotos públicas desde el volumen y lo demás al frontend | 80, 443 (TCP y UDP) | `caddy_datos`, `caddy_config`, `archivos` (solo lectura) |
| `migraciones` | Aplica las migraciones y termina (la API espera a que acabe bien) | — | — |
| `api` | Servidor HTTP | — | `archivos` |
| `worker` | Mensajes (correo y WhatsApp), recordatorios y mantenimiento nocturno | — | — |
| `db` | PostgreSQL 18 | — (nunca expuesto) | `db_datos` |
| `gotenberg` | HTML → PDF de los reportes (sin JavaScript ni red) | — | — |
| `respaldo` | Respaldos de la base cada hora y diarios con archivos; prueba de restauración los domingos | — | `respaldos`, `archivos` (solo lectura) |

El frontend se agregará en su fase; mientras tanto, lo que no sea `/api` o `/media/publico`
responde «no está disponible» (502).

Roles de la base: `diamante_propietario` (migraciones y restauraciones), `diamante_app` (la
aplicación: solo datos, sin DDL) y `diamante_respaldo` (solo lectura y crear la base desechable
de la prueba de restauración).

## 2. Primer despliegue

1. **Servidor**: Docker con Compose 2.24 o superior (los archivos usan `!override`), el dominio apuntando a la IP
   del servidor (registros A/AAAA) y los puertos 80 y 443 abiertos. Nada más expuesto (SSH con
   llave).
2. **Secretos**: `cp .env.example .env`, `chmod 600 .env` y llenar:
   - `POSTGRES_PASSWORD`, `DIAMANTE_APP_PASSWORD`, `DIAMANTE_RESPALDO_PASSWORD`: contraseñas
     largas sin caracteres especiales de URL (`openssl rand -hex 24`).
   - `DIAMANTE_ENTORNO=produccion`, `DOMINIO`, `DIAMANTE_URL_PUBLICA=https://<dominio>`.
   - `DIAMANTE_CLAVE_FIRMA` (`openssl rand -base64 48`) y `DIAMANTE_CLAVE_CIFRADO`
     (`openssl rand -base64 32`). **Guarde la clave de cifrado en el gestor de contraseñas de la
     barbería**: sin ella no se pueden leer los datos privados de los barberos, ni siquiera desde
     un respaldo.
   - SMTP (`DIAMANTE_SMTP_*`) y, cuando estén aprobadas las plantillas, WhatsApp.
   - Copia externa de respaldos: ver la sección 5.
3. **Arranque**: `docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build`.
   La configuración se valida al arrancar: si falta algo obligatorio (claves, URL pública) el
   proceso termina con el error en el log (`docker compose logs api`).
4. **Comprobar**: `curl https://<dominio>/api/salud/lista` responde `{"estado":"lista"}`.
5. **Primera administradora**:
   `docker compose run --rm api crear-administrador laura@diamante.co "Laura Gómez"`. Imprime el
   enlace de activación (y lo envía por correo si hay SMTP).

## 3. Actualizar y revertir

```bash
git pull                     # o descargar la versión etiquetada
VERSION=1.2.0 docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build
```

Las migraciones corren solas antes de la API. Antes de una actualización con migraciones,
haga un respaldo a mano (sección 4.2). Para revertir: vuelva a la etiqueta anterior; si la
versión nueva cambió el esquema de forma incompatible, restaure el respaldo previo (sección 6).

## 4. Respaldos (RNF-BAK)

### 4.1 Qué se guarda

| Cuándo | Qué | Dónde | Cuánto tiempo |
|---|---|---|---|
| Cada hora (`DIAMANTE_RESPALDO_CADA_MINUTOS`) | La base (`pg_dump` en formato propio) | `respaldos:/horario` | 48 horas |
| A diario, desde las 2:00 (`DIAMANTE_RESPALDO_HORA_DIARIA`) | La base y los archivos (fotos y documentos) | `respaldos:/diario` | 30 días (`DIAMANTE_RESPALDO_RETENCION_DIAS`) |
| Con cada respaldo, si hay remoto | Lo mismo, cifrado | `DIAMANTE_RESPALDO_REMOTO` | La misma retención |

Cada volcado lleva su suma SHA-256 y un **manifiesto** con las filas de cada tabla, tomado en
la misma instantánea que el volcado. Así la peor pérdida posible es de una hora (RPO) y la
restauración toma minutos (RTO), dentro de RNF-BAK-02.

### 4.2 Comandos

Siempre con `--no-deps`: sin él, Compose podría recrear la base si cambió su configuración.

```bash
P="docker compose -f docker-compose.yml -f docker-compose.prod.yml"
$P logs respaldo                                              # qué ha hecho
$P run --rm --no-deps respaldo respaldar.sh diario            # respaldo inmediato (antes de actualizar)
$P run --rm --no-deps respaldo probar-restauracion.sh         # probar el más reciente
$P run --rm --no-deps respaldo bash -c 'ls -lh /respaldos/*'  # listar
```

`probar-restauracion.sh` restaura en una base desechable y exige que cada tabla tenga
exactamente las filas de su manifiesto; corre sola los domingos (RNF-BAK-03). Una línea
`ERROR` en `logs respaldo` es motivo de revisión el mismo día.

### 4.3 Rol de respaldos en una base ya creada

El rol se crea solo cuando se crea el volumen de la base. En una base existente (por ejemplo,
la de desarrollo creada antes de la fase 7), créelo una vez:

```bash
docker compose exec -T db psql -U diamante_propietario -d diamante \
  -v clave="$DIAMANTE_RESPALDO_PASSWORD" <<'SQL'
CREATE ROLE diamante_respaldo LOGIN CREATEDB PASSWORD :'clave' IN ROLE pg_read_all_data;
GRANT CONNECT ON DATABASE diamante TO diamante_respaldo;
SQL
```

(o `make rol-respaldo`).

## 5. Copia fuera del servidor (RNF-BAK-01)

1. Cree `infra/respaldo/rclone/rclone.conf` (permisos `600`; está en `.gitignore`) con un
   remoto de almacenamiento y, encima, uno `crypt`. Ver
   [infra/respaldo/rclone/README.md](../../infra/respaldo/rclone/README.md).
2. En `.env`: `DIAMANTE_RESPALDO_REMOTO=cifrado:` y reinicie el servicio `respaldo`.
3. Compruebe: `$P run --rm --no-deps respaldo rclone ls cifrado:diario`.

Los respaldos salen **cifrados** (nombres y contenido): en el proveedor no se ve nada legible.
La frase del remoto `crypt` es indispensable para recuperarlos: guárdela junto a la clave de
cifrado, fuera del servidor.

## 6. Recuperación ante un desastre

### 6.1 La base

```bash
P="docker compose -f docker-compose.yml -f docker-compose.prod.yml"
$P stop api worker
$P run --rm --no-deps respaldo bash -c 'ls -1t /respaldos/*/*.dump | head'   # elegir el volcado
$P run --rm --no-deps -e PGUSER=diamante_propietario -e PGPASSWORD="$POSTGRES_PASSWORD" \
  respaldo restaurar.sh /respaldos/diario/diamante-AAAAMMDD-HHMM.dump
$P start api worker
```

`restaurar.sh` comprueba la suma SHA-256, restaura en `diamante_restaurada`, la compara tabla
por tabla con el manifiesto y **solo si coincide** la pone en lugar de la actual, que queda
como `diamante_antes_AAAAMMDDHHMM` (bórrela con `DROP DATABASE` cuando todo esté bien). Si no
coincide, la base actual no se toca. Conserva dueños y permisos: la API vuelve a funcionar con
su rol de mínimo privilegio sin pasos adicionales.

Si el servidor se perdió: en uno nuevo, despliegue (sección 2) con el mismo `.env` (mismas
claves), traiga los respaldos del remoto
(`$P run --rm --no-deps respaldo rclone copy cifrado:diario /respaldos/diario`) y restaure.

### 6.2 Los archivos (fotos y documentos)

```bash
docker run --rm -v diamante_archivos:/datos -v diamante_respaldos:/respaldos:ro debian:trixie-slim \
  tar -xzf /respaldos/diario/archivos-AAAAMMDD-HHMM.tar.gz -C /datos --strip-components=1
```

(los nombres de los volúmenes llevan el nombre del proyecto de Compose, `diamante`).

### 6.3 Probado

El procedimiento completo se probó el 21/11/2026 sobre los datos del recorrido de la colección:
respaldo diario de 46 tablas, prueba de restauración idéntica, restauración con cambio de base
(un dato agregado después del respaldo desapareció y la API leyó y escribió con su rol), y copia
externa cifrada con rclone `crypt`.

## 7. Vigilancia

- **Salud**: `GET /api/salud/lista` (la base responde). Un monitor externo (UptimeRobot o
  similar) cada 5 minutos.
- **Logs**: `docker compose logs -f api worker`. Cada petición deja una línea con método,
  **plantilla** de la ruta (nunca la URL real: no hay tokens ni datos personales), estado,
  latencia e id de solicitud (`x-request-id`, también en la respuesta). Busque `ERROR` (errores
  internos, con su causa) y `WARN` (5xx, Gotenberg caído, mensajes que no salen).
- **Avisos internos** en el panel (`/avisos`): mensajes de citas que no se entregaron y citas
  afectadas por cambios del equipo.
- **Disco**: `docker system df` y `df -h`; los respaldos locales ocupan ~2 días de horarios
  + 30 días de diarios.
- **Certificado**: Caddy lo renueva solo; si falla, lo dice en `docker compose logs proxy`.

## 8. Incidentes frecuentes

| Síntoma | Causa probable | Qué hacer |
|---|---|---|
| Los PDF responden 503 `pdf_no_disponible` | Gotenberg caído o lento | `docker compose restart gotenberg`; el resto de la aplicación sigue funcionando |
| Los correos no llegan | SMTP mal configurado o caído | `logs worker`; los correos esperan en la cola y salen solos al volver (reintentos) |
| WhatsApp no sale | Sin credenciales de Meta o plantilla rechazada | Avisos internos y `GET /mensajes`; en producción sin Meta los mensajes esperan en la cola |
| 429 `demasiados_intentos` | Límite de uso (120/min por IP en lo público, 600/min por sesión en el panel) o de inicios de sesión (10 por IP cada 15 min) | Esperar el `Retry-After`; si es tráfico legítimo detrás de una misma IP, revisar los límites |
| «Credenciales inválidas» con la contraseña correcta | Cuenta bloqueada 15 minutos tras 5 fallos | Esperar 15 minutos o «Olvidé mi contraseña» |
| La sesión pide entrar de nuevo | 12 h sin uso, 30 días con «Recordarme», o 30 días desde el inicio aunque se use | Comportamiento esperado (seguridad) |
| La API no arranca | Configuración inválida | `docker compose logs api` dice qué falta o qué está mal |

## 9. Rotación de secretos

| Secreto | Cómo rotarlo | Efecto |
|---|---|---|
| Contraseñas de la base | `ALTER ROLE … PASSWORD …` y el mismo valor en `.env`; reiniciar los servicios | Ninguno |
| `DIAMANTE_CLAVE_FIRMA` | Cambiar y reiniciar la API | Los enlaces de descarga emitidos (5 minutos) dejan de servir |
| `DIAMANTE_CLAVE_CIFRADO` | **No se rota sin migrar los datos**: cifrar de nuevo los datos privados de los barberos con la clave nueva | Con la clave equivocada esos datos no se leen |
| Secreto y token del webhook de WhatsApp | Cambiar en Meta y en `.env` a la vez | Durante el cambio, Meta reintenta los webhooks rechazados |
| Frase del remoto `crypt` | Crear un remoto nuevo y empezar a copiar allí; conservar la frase vieja mientras existan respaldos con ella | Ninguno |
