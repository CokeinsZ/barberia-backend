# Configuración de rclone para la copia externa de los respaldos

El servicio `respaldo` monta esta carpeta en `/config/rclone` (solo lectura). Para copiar los
respaldos fuera del servidor (RNF-BAK-01):

1. Cree aquí `rclone.conf` (está en `.gitignore`; permisos `600`) con un remoto de almacenamiento
   (Backblaze B2, S3, Google Drive…) y, encima, un remoto `crypt` para que los respaldos salgan
   **cifrados** (tienen datos personales de los clientes):

   ```ini
   [nube]
   type = b2
   account = …
   key = …

   [cifrado]
   type = crypt
   remote = nube:diamante-respaldos
   password = …   # rclone obscure 'frase larga'; guárdela también fuera del servidor
   ```

2. En `.env`: `DIAMANTE_RESPALDO_REMOTO=cifrado:`.
3. Pruebe: `docker compose run --rm respaldo rclone lsd cifrado:`.

Sin la frase del remoto `crypt` los respaldos externos no se pueden leer: guárdela en el gestor
de contraseñas de la barbería.
