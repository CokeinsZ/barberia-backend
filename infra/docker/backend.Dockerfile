# Imagen del backend: un solo binario (`diamante`) que cumple los papeles api, worker y migrar.
# Las migraciones van embebidas en el binario (sqlx::migrate!), así que no se copian aparte.
#
# Funciona con el constructor clásico de Docker (no exige BuildKit) y conserva la caché de
# dependencias: un cambio de código no recompila axum, sqlx ni tokio.

# 1) Receta: solo los manifiestos de Cargo, con su estructura de carpetas. Su resultado
#    cambia únicamente cuando cambian las dependencias.
FROM rust:1.97.1-trixie AS receta
WORKDIR /fuente
COPY backend/ ./
RUN mkdir /receta \
 && find . -path ./target -prune -o \( -name Cargo.toml -o -name Cargo.lock -o -name rust-toolchain.toml \) -print \
    | xargs -I{} cp --parents {} /receta

# 2) Compilación: primero un esqueleto con archivos vacíos (capa de dependencias en caché),
#    luego el código real.
FROM rust:1.97.1-trixie AS compilacion
WORKDIR /fuente
COPY --from=receta /receta ./
RUN for manifiesto in $(find crates modulos -name Cargo.toml); do \
      carpeta=$(dirname "$manifiesto"); mkdir -p "$carpeta/src"; \
      if [ "$carpeta" = "crates/app" ]; then echo 'fn main() {}' > "$carpeta/src/main.rs"; else touch "$carpeta/src/lib.rs"; fi; \
    done \
 && cargo build --release --locked --bin diamante
COPY backend/ ./
# Las fuentes copiadas conservan su fecha original; se actualizan para que Cargo recompile
# los crates propios en lugar de reutilizar el esqueleto.
RUN find crates modulos -name '*.rs' -exec touch {} + \
 && cargo build --release --locked --bin diamante \
 && cp target/release/diamante /diamante

# 3) Imagen final mínima, sin compilador y con un usuario sin privilegios.
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get upgrade -y \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin diamante \
 && mkdir -p /datos/archivos \
 && chown diamante:diamante /datos/archivos
COPY --from=compilacion /diamante /app/diamante
USER diamante
WORKDIR /app
ENV DIAMANTE_HTTP_DIRECCION=0.0.0.0:8080 \
    DIAMANTE_ARCHIVOS_DIR=/datos/archivos
# Fotos y documentos. Un volumen nuevo hereda el dueño de esta carpeta (el usuario sin privilegios).
VOLUME ["/datos/archivos"]
EXPOSE 8080
ENTRYPOINT ["/app/diamante"]
CMD ["api"]
