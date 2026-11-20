# Módulo `auditoria`

Registro de solo inserción de las acciones sensibles (RF-BAR-16, RF-SIS-07).

| | |
|---|---|
| Fase del plan | 1 ✅ |
| Esquema de base de datos | `auditoria` (ver `migraciones/`; la tabla no admite `UPDATE` ni `DELETE`) |
| Fachada | `api::Auditor<C>`: los demás módulos registran **dentro de su transacción** (ADR 0005) |
| Endpoint | `GET /api/v1/auditoria?entidad=&entidad_id=&desde=&hasta=&pagina=&por_pagina=` (administradores) |

## Cómo auditar desde otro módulo

```rust
p.auditor
    .registrar(
        &mut cx,
        EntradaAuditoria::nueva(actor, "negocio", "actualizar_datos", "negocio", 1)
            .antes(&dto_anterior)
            .despues(&dto_nuevo),
    )
    .await?;
```

`desde` y `hasta` son fechas de Bogotá; `hasta` incluye todo ese día. Los registros salen del más
reciente al más antiguo, con paginación estándar (`{ datos, pagina, por_pagina, total }`).

## Estructura

```text
auditoria/src/
├── api.rs                         # Auditor<C>, EntradaAuditoria, DTO y error
├── modulo.rs                      # auditor() para los demás módulos y rutas()
├── aplicacion/                    # ListarRegistros<C> (solo administradores) y sus puertos
├── infraestructura/postgres/      # AuditorPg y RepositorioRegistrosPg
└── http/                          # GET /auditoria
tests/registro_postgres.rs         # transacción revertida = sin rastro; filtros y paginación
```
