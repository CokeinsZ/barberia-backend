# Runbook de operación

Cómo desplegar, actualizar, respaldar, restaurar y vigilar Diamante Barbería en producción: un
VPS con **Dokploy**, que despliega desde el repositorio y tiene un **Traefik** compartido para
HTTPS y enrutamiento (ADR 0007). Escrito para quien administre el servidor.

En los comandos, `APP` es el nombre interno de la aplicación en Dokploy (el que aparece en
`docker ps`, p. ej. `diamante-backend-x1y2z3`); los contenedores se llaman `APP-servicio-1`:

```bash
APP=diamante-backend-x1y2z3      # el suyo: docker ps --format '{{.Names}}' | grep respaldo
```

## 1. Servicios

Dokploy despliega `docker-compose.yml` tal cual. Ningún servicio publica puertos: Traefik llega a
la API por la red de Dokploy.

| Servicio | Qué hace | Volúmenes |
|---|---|---|
| `migraciones` | Aplica las migraciones y termina; la API espera a que acabe bien | — |
| `api` | Servidor HTTP (`/api/*` y `/media/publico/*`) | `archivos` |
| `worker` | Mensajes (correo y WhatsApp), recordatorios y mantenimiento nocturno | — |
| `db` | PostgreSQL 18 | `db_datos` |
| `gotenberg` | HTML → PDF de los reportes (sin JavaScript ni red) | — |
| `respaldo` | Respaldos de la base cada hora y diarios con archivos; prueba de restauración los domingos | `respaldos`, `archivos` (solo lectura) |

Roles de la base: `diamante_propietario` (migraciones y restauraciones), `diamante_app` (la
aplicación: solo datos, sin DDL) y `diamante_respaldo` (solo lectura y crear la base desechable
de la prueba de restauración).

## 2. Primer despliegue en Dokploy

1. **Servidor**: conviene 4 GB de RAM (o memoria de intercambio): el backend se compila en el
   VPS en cada despliegue (Rust en modo release) y la primera vez tarda varios minutos.
2. **Aplicación**: en el proyecto, *Create Service → Compose*, conectada al repositorio y la
   rama de producción, con *Compose Path* `./docker-compose.yml` (solo ese archivo: los de
   desarrollo y E2E no se usan). Si su versión de Dokploy lo ofrece, active el despliegue
   aislado (*Isolated Deployment*): evita que nombres como `db` o `api` choquen con los de otras
   aplicaciones del servidor.
3. **Variables** (pestaña *Environment*; Dokploy las escribe en el `.env` del proyecto): ver la
   sección 3. **Todas antes del primer despliegue**: las contraseñas de los roles de la base
   (y el de respaldos) quedan fijadas cuando se crea su volumen. Si cambia una contraseña de la
   base después, hay que cambiarla también dentro (`ALTER ROLE`, sección 10) o, si la base aún
   no tiene datos, borrar el volumen `APP_db_datos` y desplegar de nuevo.
4. **Dominios** (pestaña *Domains*), con HTTPS y certificado de Let's Encrypt:

   | Servicio | Host | Path | Puerto | *Strip path* |
   |---|---|---|---|---|
   | `api` | `diamante.co` (el dominio de la barbería) | `/api` | 8080 | No |
   | `api` | el mismo | `/media` | 8080 | No |
   | `frontend` (cuando exista) | el mismo | `/` | 4321 | No |

   Traefik da prioridad a la regla más larga: `/api` y `/media` van a la API y lo demás al
   frontend. La API necesita la ruta completa (no recortar el prefijo).
5. **Desplegar** y comprobar: `curl https://diamante.co/api/salud/lista` responde
   `{"estado":"lista"}`. Si falta una variable obligatoria, la API no arranca y lo dice en sus
   logs (pestaña *Logs* o `docker logs APP-api-1`).
6. **Primera administradora**:
   `docker exec APP-api-1 /app/diamante crear-administrador laura@diamante.co "Laura Gómez"`.
   Imprime el enlace de activación (y lo envía por correo si hay SMTP).
7. **Probar la preproducción** con la batería E2E contra el servidor desplegado
   ([pruebas/preproduccion](../../pruebas/preproduccion/README.md)). Deja datos marcados
   `E2E …` y mensajes de WhatsApp en cola: antes de abrir al público, reinicie la base.

## 3. Variables de producción

Generar los valores en su equipo y pegarlos en *Environment*. Las contraseñas pueden tener
cualquier carácter: `docker-compose.yml` las pasa en `PGPASSWORD`, no dentro de la URL de la
base. **Cada variable con su propio valor** (no reutilice la misma clave).

| Variable | Valor | Cómo generarla |
|---|---|---|
| `POSTGRES_PASSWORD` | Rol propietario (migraciones, restauraciones) | `openssl rand -base64 32` |
| `DIAMANTE_APP_PASSWORD` | Rol de la aplicación | `openssl rand -base64 32` |
| `DIAMANTE_RESPALDO_PASSWORD` | Rol de los respaldos | `openssl rand -base64 32` |
| `DIAMANTE_ENTORNO` | `produccion` (es el valor por defecto) | — |
| `DIAMANTE_URL_PUBLICA` | `https://diamante.co` (sin barra final) | — |
| `DIAMANTE_CONFIAR_PROXY` | `true`: la IP del cliente viene de Traefik | — |
| `DIAMANTE_CLAVE_FIRMA` | Firma de las URL de descarga | `openssl rand -base64 48` |
| `DIAMANTE_CLAVE_CIFRADO` | Cifrado de los datos privados de los barberos | `openssl rand -base64 32` |
| `DIAMANTE_SMTP_HOST`, `_PUERTO`, `_SEGURIDAD`, `_USUARIO`, `_CLAVE`, `_REMITENTE` | Correo saliente (invitaciones, recuperación). Puerto 465 con `tls` o 587 con `starttls` (la combinación cruzada no arranca). Gmail: clave de aplicación sin espacios | Del proveedor de correo |
| `DIAMANTE_RESPALDO_REMOTO` | `cifrado:` | Sección 5 |
| `DIAMANTE_RESPALDO_RCLONE_CONF_B64` | Configuración de rclone en base64 | Sección 5 |

Opcionales (tienen buen valor por defecto): `DIAMANTE_RESPALDO_CADA_MINUTOS` (60),
`DIAMANTE_RESPALDO_HORA_DIARIA` (2), `DIAMANTE_RESPALDO_RETENCION_DIAS` (30),
`DIAMANTE_ORIGENES` (otros orígenes permitidos), `DIAMANTE_WHATSAPP_SECRETO_APP` y
`DIAMANTE_WHATSAPP_TOKEN_VERIFICACION` (cuando estén las plantillas de Meta). **Nunca en
producción**: `DIAMANTE_PRUEBAS_E2E`, `DIAMANTE_WORKER_EN_API` ni `DIAMANTE_LIMITES_DE_USO=false`
(la API se niega a arrancar con los dos que lo comprueban).

**Guarde en el gestor de contraseñas de la barbería, fuera del servidor**: `DIAMANTE_CLAVE_CIFRADO`
(sin ella no se leen los datos privados de los barberos, ni desde un respaldo) y las frases de
rclone (sin ellas no se leen los respaldos externos).

## 4. Actualizar y revertir

Dokploy despliega con cada *push* a la rama (o con *Deploy*): construye las imágenes, corre las
migraciones y reemplaza los contenedores. Antes de un cambio con migraciones, haga un respaldo
(`docker exec APP-respaldo-1 respaldar.sh diario`). Para revertir: despliegue el commit anterior
(*Deployments* en Dokploy); si la versión nueva cambió el esquema de forma incompatible,
restaure el respaldo previo (sección 6).

**Nunca** borre la aplicación en Dokploy marcando la eliminación de volúmenes: ahí están la
base, los archivos y los respaldos locales.

## 5. Respaldos (RNF-BAK)

### 5.1 Cómo funcionan

El servicio `respaldo` corre todo el tiempo y hace esto solo:

| Cuándo | Qué | Dónde | Cuánto tiempo |
|---|---|---|---|
| Cada hora | La base (`pg_dump` en formato propio) | volumen `respaldos`, carpeta `horario` | 48 horas |
| Una vez al día, desde las 2:00 a. m. | La base y los archivos (fotos y documentos) | carpeta `diario` | 30 días |
| Con cada respaldo, si hay remoto | Lo mismo, **cifrado** | el remoto de rclone | La misma retención |
| Los domingos, después del diario | La prueba de restauración | base desechable | — |

- Cada volcado lleva su suma SHA-256 y un **manifiesto** con las filas de cada tabla, tomado en
  la misma instantánea que el volcado.
- La **prueba de restauración** restaura el último volcado en una base desechable, exige que cada
  tabla tenga exactamente las filas del manifiesto y la borra (RNF-BAK-03).
- A lo sumo se pierde una hora de datos (RPO) y una restauración toma minutos (RTO): RNF-BAK-02.
- Los respaldos locales están en el **mismo servidor**: sin la copia externa, si se pierde el VPS
  se pierde todo. Configurarla es obligatorio (RNF-BAK-01).

### 5.2 Copia externa cifrada

1. Elija un almacenamiento (Backblaze B2 es barato y simple; también S3, Google Drive…).
2. En su equipo, escriba `rclone.conf` con el almacenamiento y un remoto `crypt` encima (ejemplo
   en [infra/respaldo/rclone/README.md](../../infra/respaldo/rclone/README.md)). Las frases del
   `crypt` se ofuscan con `docker run --rm rclone/rclone obscure 'frase larga'`.
3. En *Environment*: `DIAMANTE_RESPALDO_REMOTO=cifrado:` y
   `DIAMANTE_RESPALDO_RCLONE_CONF_B64=` + la salida de `base64 -w0 rclone.conf`. Despliegue.
4. Compruebe: `docker exec APP-respaldo-1 remoto.sh lsd cifrado:` y, tras el siguiente respaldo,
   `docker exec APP-respaldo-1 remoto.sh ls cifrado:horario`.

En el proveedor solo se ven nombres y contenidos cifrados.

### 5.3 Comandos

```bash
docker logs APP-respaldo-1                                   # qué ha hecho (busque ERROR)
docker exec APP-respaldo-1 respaldar.sh diario               # respaldo inmediato
docker exec APP-respaldo-1 probar-restauracion.sh            # probar el más reciente
docker exec APP-respaldo-1 bash -c 'ls -lh /respaldos/*'     # listar
```

Una línea `ERROR` en el log del respaldo se revisa el mismo día.

### 5.4 Rol de respaldos en una base ya creada

El rol se crea con el volumen de la base. Si la base ya existía (p. ej. la de desarrollo, o si
la variable se agregó después), créelo una vez:

```bash
docker exec -i APP-db-1 psql -U diamante_propietario -d diamante -v clave='la-clave-de-respaldo' <<'SQL'
CREATE ROLE diamante_respaldo LOGIN CREATEDB PASSWORD :'clave' IN ROLE pg_read_all_data;
GRANT CONNECT ON DATABASE diamante TO diamante_respaldo;
SQL
```

En desarrollo: `make rol-respaldo`.

## 6. Recuperación ante un desastre

### 6.1 La base

```bash
docker exec APP-respaldo-1 bash -c 'ls -1t /respaldos/*/*.dump | head'     # elegir el volcado
docker stop APP-api-1 APP-worker-1
docker exec -e PGUSER=diamante_propietario \
  -e PGPASSWORD="$(docker exec APP-db-1 printenv POSTGRES_PASSWORD)" \
  APP-respaldo-1 restaurar.sh /respaldos/diario/diamante-AAAAMMDD-HHMM.dump
docker start APP-api-1 APP-worker-1
```

`restaurar.sh` comprueba la suma SHA-256, restaura en `diamante_restaurada`, la compara tabla
por tabla con el manifiesto y **solo si coincide** la pone en lugar de la actual, que queda
como `diamante_antes_AAAAMMDDHHMM` (bórrela con `DROP DATABASE` cuando todo esté bien). Si no
coincide, la base actual no se toca. Conserva dueños y permisos: la API vuelve a funcionar con
su rol de mínimo privilegio sin pasos adicionales.

**Si se perdió el servidor**: en uno nuevo, cree la aplicación en Dokploy con **las mismas
variables** (sobre todo `DIAMANTE_CLAVE_CIFRADO`, `DIAMANTE_CLAVE_FIRMA` y las de rclone),
despliegue, traiga los respaldos (`docker exec APP-respaldo-1 remoto.sh copy cifrado:diario
/respaldos/diario`) y restaure como arriba.

### 6.2 Los archivos (fotos y documentos)

```bash
docker run --rm -v APP_archivos:/datos -v APP_respaldos:/respaldos:ro debian:trixie-slim \
  tar -xzf /respaldos/diario/archivos-AAAAMMDD-HHMM.tar.gz -C /datos --strip-components=1
```

(`docker volume ls | grep archivos` muestra el nombre exacto del volumen).

### 6.3 Probado

El 21/11/2026, sobre los datos del recorrido completo de la colección: respaldo diario de 46
tablas, prueba de restauración idéntica, restauración con cambio de base (un dato agregado
después del respaldo desapareció y la API leyó y escribió con su rol) y copia externa cifrada
con rclone `crypt` (configurada por archivo y por la variable en base64).

## 7. Traefik de Dokploy

La configuración de Traefik es del servidor (`/etc/dokploy/traefik/`) y la comparten todas las
aplicaciones. No hay que agregarle nada para que Diamante funcione; estas recomendaciones son
de seguridad:

1. **Log de acceso**: `accessLog` guarda la URL completa de cada petición, incluidas las de los
   enlaces de gestión de citas (con su token, que permite cancelar o reprogramar) y las
   búsquedas del panel (`?q=` con nombres o celulares). La API ya no registra nada de eso. Si se
   quiere el mismo cuidado en Traefik, omita la ruta en `traefik.yml` (afecta a todas las
   aplicaciones del servidor):

   ```yaml
   accessLog:
     filePath: /etc/dokploy/traefik/dynamic/access.log
     format: json
     bufferingSize: 100
     fields:
       names:
         RequestPath: drop
   ```

   Si se deja como está, que el archivo no salga del servidor y se rote.
2. **Panel de Traefik**: `api.insecure: true` abre el panel sin contraseña en el puerto 8080 del
   contenedor. Compruebe que no esté publicado
   (`docker ps --filter name=traefik --format '{{.Ports}}'` no debe mostrar `8080`); si lo está,
   ciérrelo en el firewall o quite `insecure`.
3. **Cloudflare**: si el dominio pasa por el proxy de Cloudflare (nube naranja), Traefik verá
   las IP de Cloudflare y los límites por IP contarían a muchos clientes como uno. En ese caso
   agregue los rangos de Cloudflare en `entryPoints.websecure.forwardedHeaders.trustedIPs`.
4. **HSTS**: la API ya lo envía. Para todo el dominio (frontend incluido) se puede agregar un
   middleware `headers` con `stsSeconds: 31536000` en `dynamic/middlewares.yml`.

## 8. Vigilancia

- **Salud**: `GET /api/salud/lista` (la base responde). Un monitor externo (UptimeRobot o
  similar) cada 5 minutos.
- **Logs**: pestaña *Logs* de Dokploy o `docker logs -f APP-api-1` / `APP-worker-1`. Cada
  petición deja una línea con método, **plantilla** de la ruta (nunca la URL real: no hay tokens
  ni datos personales), estado, latencia e id de solicitud (`x-request-id`, también en la
  respuesta). Busque `ERROR` (errores internos, con su causa) y `WARN` (5xx, Gotenberg caído,
  mensajes que no salen).
- **Avisos internos** en el panel (`/avisos`): mensajes de citas que no se entregaron y citas
  afectadas por cambios del equipo.
- **Disco**: `df -h` y `docker system df`; los respaldos locales ocupan 2 días de horarios y 30
  de diarios. Dokploy acumula imágenes de despliegues anteriores: límpielas de vez en cuando.
- **Certificados**: los renueva Traefik (Dokploy).

## 9. Incidentes frecuentes

| Síntoma | Causa probable | Qué hacer |
|---|---|---|
| Los PDF responden 503 `pdf_no_disponible` | Gotenberg caído o lento | `docker restart APP-gotenberg-1`; el resto sigue funcionando |
| Los correos no llegan | SMTP mal configurado o caído | `docker logs APP-worker-1`; los correos esperan en la cola y salen al volver |
| WhatsApp no sale | Sin credenciales de Meta o plantilla rechazada | Avisos internos y `GET /mensajes`; sin Meta, los mensajes esperan en la cola |
| 429 `demasiados_intentos` | Límite de uso (120/min por IP en lo público, 600/min por sesión en el panel) o de inicios de sesión (10 por IP cada 15 min) | Esperar el `Retry-After`; si es tráfico legítimo detrás de una misma IP, revisar los límites y la nota de Cloudflare |
| «Credenciales inválidas» con la contraseña correcta | Cuenta bloqueada 15 minutos tras 5 fallos | Esperar 15 minutos o «Olvidé mi contraseña» |
| Un enlace de activación o de recuperación no llegó o venció | Correo caído, dirección mal escrita o pasó la hora | `docker exec APP-api-1 /app/diamante enlace-acceso <correo>`: imprime un enlace nuevo (activación si la cuenta no está activada, recuperación si ya lo está), sin enviar correo, y queda en la auditoría |
| La sesión pide entrar de nuevo | 12 h sin uso, 30 días con «Recordarme», o 30 días desde el inicio aunque se use | Comportamiento esperado (seguridad) |
| La API, el worker o las migraciones no arrancan | Configuración inválida | Sus logs dicen qué falta o qué está mal (`Configuración inválida. Faltan: … Inválidas: …`) |
| `password authentication failed` | La contraseña de *Environment* no es la que tiene el rol en la base (se fijó al crear el volumen) | `ALTER ROLE` con la nueva, o borrar el volumen si aún no hay datos |
| El despliegue falla al compilar | Memoria insuficiente en el VPS | Agregar memoria de intercambio o RAM |

## 10. Rotación de secretos

| Secreto | Cómo rotarlo | Efecto |
|---|---|---|
| Contraseñas de la base | `ALTER ROLE … PASSWORD …` en la base y el mismo valor en *Environment*; desplegar | Ninguno |
| `DIAMANTE_CLAVE_FIRMA` | Cambiar y desplegar | Los enlaces de descarga emitidos (5 minutos) dejan de servir |
| `DIAMANTE_CLAVE_CIFRADO` | **No se rota sin migrar los datos**: cifrar de nuevo los datos privados de los barberos con la clave nueva | Con la clave equivocada esos datos no se leen |
| Secreto y token del webhook de WhatsApp | Cambiar en Meta y en *Environment* a la vez | Durante el cambio, Meta reintenta los webhooks rechazados |
| Frases del remoto `crypt` | Crear un remoto nuevo y copiar allí; conservar las frases viejas mientras existan respaldos con ellas | Ninguno |
