# Revisión de seguridad del backend (OWASP ASVS 4.0.3, nivel 1)

Revisión de la fase 7, hecha el 21/11/2026 sobre el backend (API, worker, base de datos e
infraestructura de Docker Compose). El frontend se revisará en su fase. Cada control dice cómo
está implementado y qué prueba automática lo respalda.

## Resumen

- **Corregido en esta revisión**:
  1. Con «Cualquier barbero», dos reservas simultáneas podían responder `500`: era un bloqueo
     mutuo de PostgreSQL en la restricción de exclusión de las citas. Ahora se reintenta (V11).
  2. Las sesiones no tenían vida máxima: con «Recordarme» y uso diario no vencían nunca. Ahora
     vencen a los 30 días de iniciarse (V3.3.2).
  3. Faltaban las cabeceras de seguridad en la API (HSTS incluido, al retirar Caddy), los límites
     de uso generales y una traza de peticiones sin datos personales (V14.4, V11.1.4, V7.1).
- **Pendiente** (sección final): revisar contraseñas contra listas de contraseñas filtradas
  (V2.1.7), el reenvío de la IP del cliente desde el servidor del frontend y la política de
  contenido (CSP) de las páginas, que es del frontend.
- **Dependencias**: `cargo audit` sobre 275 crates sin vulnerabilidades conocidas. Dos avisos
  «sin mantenimiento» (`chrono`, `chrono-tz`, RUSTSEC-2026-0543 y 0555) sin versión de
  reemplazo: se vigilan; no son fallas explotables.

## V1 · Arquitectura

| Control | Implementación |
|---|---|
| Separación de módulos y mínimo acoplamiento | Monolito modular (ADR 0001): cada módulo con su esquema; `reportes` solo lee vistas publicadas |
| Mínimo privilegio en la base | `diamante_app` solo DML (sin DDL); `diamante_respaldo` solo lectura; las migraciones con `diamante_propietario`. La base no publica puertos |
| Servicios internos aislados | Ningún servicio publica puertos: el Traefik de Dokploy (80/443) llega a la API por su red. Gotenberg sin JavaScript ni red: se comprobó que bloquea `http://example.com` y `http://169.254.169.254` (SSRF) |

## V2 · Autenticación

| Control | Implementación | Prueba |
|---|---|---|
| Contraseñas de 12 a 128 caracteres, cualquier carácter | `identidad::dominio::contrasena` | unitarias de `identidad` |
| Almacenamiento con Argon2id | `plataforma::seguridad::contrasenas` | unitarias |
| Bloqueo tras 5 fallos (15 min) y 10 intentos por IP cada 15 min | `IniciarSesion` (fila del usuario con `FOR UPDATE`: ningún fallo se pierde en paralelo) | `cinco_fallos_bloquean…`, `carreras_de_clientes…` (6 fallos simultáneos) |
| Mensaje genérico y mismo tiempo de respuesta exista o no la cuenta | Se verifica un hash ficticio cuando la cuenta no existe | unitarias de `identidad` |
| Recuperación: token de 256 bits, una hora, un solo uso; cierra las demás sesiones | `identidad` + correo | `recuperacion_de_contrasena_cierra_las_sesiones…` |
| Cambio de contraseña pide la actual y cierra las demás sesiones | `CambiarContrasena` | unitarias e integración |
| Código de verificación del cliente: 4 dígitos con HMAC, 5 intentos, espera de reenvío, 3 por celular y 10 por IP | `verificacion` | E2E de reserva |

## V3 · Sesiones

| Control | Implementación | Prueba |
|---|---|---|
| Cookie `__Host-sesion`, `HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/` | `plataforma::http::cookies` | unitarias |
| Token aleatorio de 256 bits; en la base solo su SHA-256 | `generar_token`, `identidad.sesion.token_hash` | unitarias |
| Se valida en cada petición (revocada, vencida o cuenta sin acceso deja de servir de inmediato) | `ValidarSesion` | `un_barbero_suspendido_no_entra…` |
| Vence tras 12 h sin uso (30 días con «Recordarme») y **siempre a los 30 días de iniciada** | `DuracionSesion::vence_en` | `ni_usandola_a_diario_una_sesion_dura_mas_de_30_dias` |
| Cerrar sesión la revoca en el servidor | `CerrarSesion` | `cerrar_sesion_invalida_el_token` |

## V4 · Control de acceso

| Control | Implementación | Prueba |
|---|---|---|
| Negar por defecto: cada caso de uso exige su rol antes de leer o escribir | Políticas en cada módulo (`solo_administrador`, «lo ajeno no existe» para el barbero) | `matriz_de_autorizacion_de_todas_las_rutas` |
| Ninguna ruta del panel sin sesión | — | La matriz recorre **todas** las rutas del código (falla si aparece una sin clasificar): sin sesión, `401` |
| El barbero no alcanza nada de la administradora, ni lecturas ni escrituras | — | Matriz: `403`/`404` en cada ruta de administrador, y luego se comprueba que nada cambió |
| Sin IDOR entre barberos | Filtros por el barbero de la sesión en citas, ventas, horario, bloqueos, reportes | Matriz: Santiago contra recursos de Mateo → `403`/`404`; los listados solo traen lo suyo |
| CSRF | `SameSite=Strict` + verificación de `Origin` en toda petición que modifica | `rechaza_peticiones_que_modifican_datos_desde_otro_origen` |

## V5 · Validación y codificación

| Control | Implementación |
|---|---|
| Entradas tipadas y estrictas | `serde` con `deny_unknown_fields`; parámetros de consulta desconocidos → `400`; validación de dominio con errores por campo (`422`) |
| SQL sin inyección | Consultas parametrizadas (`sqlx`); el SQL dinámico solo concatena constantes del propio archivo (`AssertSqlSafe`) |
| Salida | La API responde JSON. El HTML de los PDF escapa todo dato (`pdf::escapar`, probado con `<script>`) y Gotenberg no ejecuta JavaScript |
| Archivos | Tipo detectado por el contenido, no por el nombre; límites de tamaño por ruta (foto 6 MB, documento 11 MB); claves de archivo validadas (sin `..`) |

## V7 · Errores y logs

| Control | Implementación | Prueba |
|---|---|---|
| Errores sin detalles internos | *Problem Details* (RFC 9457); los `500` dicen «Ocurrió un error inesperado» y la causa va solo al log | unitarias de `plataforma` |
| Logs sin datos personales ni secretos | Cada petición registra la **plantilla** de la ruta (`/publico/citas/gestion/{token}`), nunca la URL: ni tokens de gestión, ni búsquedas (`?q=`), ni firmas (`?firma=`). El log de acceso de Traefik es del servidor: el runbook explica cómo omitir la ruta (verificado: sin tokens ni celulares) | `la_traza_lleva_la_plantilla_de_la_ruta_y_no_la_url`; en el recorrido E2E, 0 tokens o celulares en los logs |
| Secretos que no aparecen en `Debug` | `Secreto`, `ClaveCifrado` | unitarias de configuración |
| Auditoría inalterable | `auditoria.registro` solo admite inserciones (trigger); cubre exportaciones de reportes y acciones sensibles | pruebas de esquema y E2E |

Excepción conocida: fuera de producción, el adaptador simulado de WhatsApp escribe el texto de
cada mensaje en el log (incluye códigos y enlaces) para poder probar sin Meta. En producción
no existe.

## V8 · Protección de datos

| Control | Implementación |
|---|---|
| Sin caché de datos del panel | `Cache-Control: no-store` por defecto en la API; solo la landing pública (60 s) y las fotos públicas se guardan |
| Datos sensibles cifrados | Datos privados de los barberos con AES-256-GCM (`DIAMANTE_CLAVE_CIFRADO`) |
| Derechos del titular (Ley 1581) | Anonimización y solicitudes con plazo legal (`clientes`) |
| Respaldos | Copia externa cifrada (rclone `crypt`), restauración probada |

## V9 · Comunicaciones

HTTPS obligatorio en el Traefik de Dokploy (Let's Encrypt, redirección desde HTTP, TLS 1.2 o
superior con suites modernas). La API envía HSTS de un año cuando la URL pública es `https`. El
tráfico entre contenedores va por las redes internas de Docker. Verificado con un Traefik 3.6
simulado (ADR 0007).

## V10 · Código malicioso y dependencias

`Cargo.lock` versionado y compilación con `--locked`; `cargo audit` sin vulnerabilidades.
Imágenes fijadas por versión (`gotenberg/gotenberg:8.40.0`,
`axllent/mailpit:v1.27.0`, `rust:1.97.1`); `postgres:18` sigue la versión menor (parches de
seguridad) dentro de la mayor.

## V11 · Lógica de negocio

| Control | Implementación | Prueba |
|---|---|---|
| Límites de uso | 120/min por IP en lo público, 600/min por sesión y 1.200/min por IP en el panel, 120/min sin sesión; `429` con `Retry-After`. Inventar cookies o falsificar `X-Forwarded-For` no los evita | unitarias; en Docker, 120 peticiones pasan y el resto recibe `429` (también con `X-Forwarded-For` falsos detrás de Traefik) |
| Sin dobles reservas ni cobros dobles | Restricción de exclusión en `agenda.cita`, `UNIQUE (cita_id)` en ventas, contador de tiquetes bloqueado | `reservas_y_ventas_simultaneas` |
| Carreras | Reserva en línea por la misma hora, celulares repetidos, métricas del cliente con ventas simultáneas, anulaciones y cierres dobles, transiciones cruzadas de una cita, varios workers a la vez | `carreras_de_clientes_ventas_agenda_y_worker` (repetidas 15 veces sin fallar) |

## V12 · Archivos

Documentos privados solo con URL firmada (HMAC-SHA256, 5 minutos), siempre como adjunto y con
`nosniff`. `/media/{*ruta}` solo lee claves de `publico/` validadas (sin `..`); lo privado
responde `404`.

## V13 · API y webhooks

Webhook de WhatsApp con firma `X-Hub-Signature-256` sobre el cuerpo exacto y comparación en
tiempo constante; el token de verificación de Meta también en tiempo constante. Sin CORS: el
sitio y la API comparten origen detrás de Traefik.

## V14 · Configuración

| Control | Implementación |
|---|---|
| Cabeceras | API: `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy`, `CSP: default-src 'none'; frame-ancestors 'none'`, `X-Robots-Tag: noindex` y HSTS en HTTPS. TLS y certificados en Traefik |
| Producción no arranca mal configurada | Exige URL pública, claves de firma y cifrado; rechaza `DIAMANTE_PRUEBAS_E2E=true` y `DIAMANTE_LIMITES_DE_USO=false` |
| Tiempo máximo por petición | 30 s (120 s los reportes por los PDF) |

## Pendientes y riesgos aceptados

1. **Contraseñas filtradas (V2.1.7)**: no se comparan contra una lista de contraseñas
   comprometidas. Propuesta: incluir en el binario una lista de las ~100.000 más comunes
   (sin llamadas externas, para no enviar datos fuera).
2. **IP del cliente con el frontend en el servidor (SSR)**: cuando Astro llame a la API desde
   el servidor, debe reenviar la IP del navegador; si no, los límites por IP contarían a todos
   los visitantes como uno solo. Se resuelve en la fase del frontend.
3. **CSP de las páginas**: la API ya la envía; las páginas del sitio la definirán en el frontend.
4. **Log de acceso y panel de Traefik**: son del servidor (Dokploy); ver la sección 7 del
   runbook (omitir la ruta del log, no publicar el panel en el puerto 8080).
5. **Segundo factor para administradores**: previsto como requisito *Could* (fase 9).
6. **`chrono` sin mantenimiento**: vigilar el aviso de RustSec y evaluar la migración a `jiff`
   si aparece una vulnerabilidad.
