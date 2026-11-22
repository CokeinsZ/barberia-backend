# Configuración de rclone para la copia externa de los respaldos

La copia fuera del servidor (RNF-BAK-01) usa rclone. Se configura de una de dos formas:

- **Archivo**: `infra/respaldo/rclone/rclone.conf` (está en `.gitignore`; permisos `600`). El
  servicio `respaldo` monta esta carpeta en `/config/rclone`. Sirve en un servidor manejado a
  mano.
- **Variable** (Dokploy y otros paneles que solo manejan variables de entorno):
  `DIAMANTE_RESPALDO_RCLONE_CONF_B64` con el contenido del mismo archivo en base64
  (`base64 -w0 rclone.conf`). Dentro del contenedor se escribe en un archivo privado.

## El archivo

Un remoto de almacenamiento (Backblaze B2, S3, Google Drive…) y, encima, uno `crypt` para que
los respaldos salgan **cifrados**: tienen datos personales de los clientes.

```ini
[nube]
type = b2
account = …
key = …

[cifrado]
type = crypt
remote = nube:diamante-respaldos
password = …    # rclone obscure 'frase larga'
password2 = …   # rclone obscure 'otra frase larga' (sal)
```

Las frases se ofuscan con `rclone obscure` (con rclone instalado, o
`docker run --rm rclone/rclone obscure 'frase larga'`). **Guarde las frases originales en el
gestor de contraseñas de la barbería**: sin ellas los respaldos externos no se pueden leer.

## Activarlo y probarlo

1. `DIAMANTE_RESPALDO_REMOTO=cifrado:` (y la variable en base64 si no usa el archivo).
2. Pruebe la conexión:
   `docker compose run --rm --no-deps respaldo remoto.sh lsd cifrado:`.
3. Después del siguiente respaldo: `… respaldo remoto.sh ls cifrado:horario`.
