# Servicio de respaldos (RNF-BAK): pg_dump de la misma versión mayor que el servidor, rclone
# para la copia fuera del servidor y los scripts de infra/respaldo. Corre como `postgres`
# (sin privilegios) y se conecta con el rol de solo lectura diamante_respaldo.
FROM postgres:18
RUN apt-get update \
 && apt-get install -y --no-install-recommends rclone ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /respaldos /config/rclone \
 && chown postgres:postgres /respaldos
COPY infra/respaldo/*.sh /usr/local/bin/
ENV TZ=America/Bogota \
    RCLONE_CONFIG=/config/rclone/rclone.conf
USER postgres
WORKDIR /respaldos
VOLUME ["/respaldos"]
CMD ["programar.sh"]
