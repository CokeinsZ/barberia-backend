# Diamante Barbería

Plataforma web de reservas y administración para Diamante Barbería: sitio público con
agendamiento en línea, panel del administrador y panel del barbero.

| Parte | Tecnología | Carpeta | Estado |
|---|---|---|---|
| Backend | Rust + Axum, monolito modular | [`backend/`](backend/) | Fases 0 a 5 listas: fundaciones, `identidad`, `negocio`, `auditoria`, `archivos`, `catalogo`, `barberos`, `clientes`, `verificacion`, `agenda`, `notificaciones` (correo, WhatsApp simulado hasta tener Meta, webhooks y avisos), `trabajos` (recordatorios y mantenimiento) y `ventas` (caja y cierres) |
| Base de datos | PostgreSQL 18, un esquema por módulo | `backend/**/migraciones/` | Esquema completo (45 tablas) en migraciones por módulo |
| Frontend | Astro + React | `frontend/` | Pendiente (fase 0, parte de frontend) |
| Pruebas E2E | Playwright | `e2e/` | Pendiente |
| Infraestructura | Docker Compose | [`infra/`](infra/), `docker-compose*.yml` | Base de datos, migraciones, API (con el volumen de archivos), worker y Mailpit (desarrollo y E2E) |

## Documentación

- [Documento de visión](docs/documento-vision.md)
- [Plan de desarrollo](docs/plan-de-desarrollo.md)
- Diseño: [base de datos](docs/diseno/base-de-datos.md), [API](docs/diseno/api.md), [pruebas E2E](docs/diseno/pruebas-e2e.md)
- [Decisiones de arquitectura (ADR)](docs/decisiones/)
- Módulo de referencia: [`backend/modulos/negocio`](backend/modulos/negocio/README.md)
- Pruebas manuales: [colección de Postman](docs/pruebas-manuales/diamante-barberia.postman_collection.json) (305 peticiones por módulo, verificada con newman)

## Arranque rápido

```bash
cp .env.example .env        # y cambie las contraseñas
make dev                    # base de datos + migraciones + API + worker + Mailpit
curl localhost:8080/api/salud/lista
make administrador CORREO=laura@diamante.co NOMBRE="Laura Gómez"   # primer administrador
```

La invitación muestra el enlace de activación en la terminal y llega también a Mailpit
(<http://localhost:8025>). El enlace apunta al frontend (`/acceso/activar#token=…`); mientras
el frontend no exista, la cuenta se activa con `POST /api/v1/auth/activacion`.

`make ayuda` lista todos los atajos (`backend-lint`, `backend-pruebas`, `verificar-esquema`…).

## Estructura del backend

```text
backend/
├── crates/
│   ├── nucleo/         # tipos de dominio compartidos (dinero, celular, reloj, eventos)
│   ├── contratos/      # eventos entre módulos
│   ├── plataforma/     # configuración, PostgreSQL, HTTP, telemetría; migración base
│   └── app/            # raíz de composición y binario `diamante` (api | worker | migrar | salud | crear-administrador)
├── modulos/            # 13 módulos, cada uno con su esquema, sus capas y sus pruebas
└── scripts/            # verificar-esquema.sh, estructurar-modulo.sh
```

Reglas de modularidad, capas y patrones: [plan de desarrollo, secciones 3 a 6](docs/plan-de-desarrollo.md).
