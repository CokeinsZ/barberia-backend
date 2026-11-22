# ADR 0007 — Despliegue con Dokploy y su Traefik (sin Caddy)

**Estado**: aceptada · **Fecha**: 21/11/2026 · **Reemplaza**: el proxy Caddy del plan (sección 7)

## Contexto

El plan preveía un servicio `proxy` con Caddy en el Docker Compose de producción (HTTPS
automático, enrutamiento, cabeceras y fotos públicas desde el volumen). El equipo despliega
todo (backend y frontend) en un VPS con **Dokploy**, que hace el despliegue continuo desde el
repositorio y ya tiene un **Traefik** compartido por todas las aplicaciones del servidor, con
Let's Encrypt, redirección a HTTPS y TLS 1.2 o superior. Dos proxies en cadena no aportan nada y
Caddy no podría usar los puertos 80 y 443, que son de Traefik.

## Decisión

- Se retira Caddy (`docker-compose.prod.yml` e `infra/caddy/`). Dokploy despliega
  `docker-compose.yml` tal cual; Traefik termina TLS y enruta por los dominios configurados en
  Dokploy: `/api` y `/media` a la API y, cuando exista, el resto al frontend.
- Lo que hacía Caddy queda así:
  - **HTTPS, certificados y redirección**: Traefik (Dokploy).
  - **HSTS**: la API lo envía cuando `DIAMANTE_URL_PUBLICA` es `https` (y el frontend hará lo
    mismo). Las demás cabeceras de seguridad ya las enviaba la API.
  - **Fotos públicas**: las sirve la API (`/media/publico/*`), con caché de un año.
  - **IP del cliente**: Traefik reemplaza `X-Forwarded-For` con la IP real cuando la petición no
    viene de un proxy de confianza; la API toma el último valor (`DIAMANTE_CONFIAR_PROXY=true`).
  - **Límite de tamaño**: los límites de la API por ruta (foto 6 MB, documento 11 MB).
- La configuración de rclone para la copia externa de los respaldos puede ir en una variable
  (`DIAMANTE_RESPALDO_RCLONE_CONF_B64`), porque Dokploy maneja variables, no archivos secretos.

## Consecuencias

- Un componente menos que mantener y un solo lugar para los certificados del servidor.
- El log de acceso de Traefik es global: registra las URL completas de todas las aplicaciones,
  incluidas las de los enlaces de gestión de citas (con su token) y las búsquedas del panel. Se
  decide en la configuración de Traefik (ver el runbook) si se omite la ruta.
- El build del backend corre en el VPS en cada despliegue (Rust en modo release): conviene un
  servidor con 4 GB de RAM o memoria de intercambio.
