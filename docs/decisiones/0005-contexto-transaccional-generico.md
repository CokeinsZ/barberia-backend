# ADR 0005 — Puertos genéricos en el contexto transaccional

**Estado**: aceptada · **Fecha**: 20/11/2026

## Contexto

Varias operaciones deben confirmarse juntas aunque las escriban módulos distintos: un cambio
del negocio y su registro de auditoría; una invitación, su enlace y el correo que la lleva. Si
el correo se encola en otra transacción, puede salir un correo de una invitación que se revirtió,
o perderse el correo de una que se confirmó.

A la vez, los casos de uso no deben conocer SQLx, y sus pruebas deben correr en memoria.

## Decisión

Los puertos son genéricos en un **contexto** `C` y reciben `&mut C` en cada operación:

```rust
#[async_trait]
pub trait Auditor<C: Send>: Send + Sync + fmt::Debug {
    async fn registrar(&self, contexto: &mut C, entrada: EntradaAuditoria) -> Result<(), ErrorPersistencia>;
}
```

- `nucleo::Transacciones<C>` abre y confirma el contexto. Si el caso de uso termina con error
  antes de confirmar, el contexto se descarta y la transacción se revierte.
- **Producción**: `C = plataforma::base_datos::UnidadDeTrabajo` (una transacción de PostgreSQL),
  con `TransaccionesPg`.
- **Pruebas**: `C = ()` con `nucleo::SinTransaccion` y dobles en memoria.
- Las **fachadas entre módulos** siguen la misma regla (`Auditor<C>`, `BuzonSalida<C>`,
  `PerfilesBarberos<C>`, `CalendarioNegocio<C>`): quien llama pasa su contexto y todo queda en
  su transacción.
- Cada módulo agrupa sus puertos en una estructura `Puertos<C>` que comparten sus casos de uso,
  para no repetir la lista de dependencias en cada constructor.
- La capa HTTP de cada módulo también es genérica en `C`; `modulo.rs` la instancia con
  `UnidadDeTrabajo`.

## Consecuencias

- Atomicidad entre módulos sin que ninguno conozca el esquema del otro (patrón *outbox* para
  los correos).
- Los casos de uso se prueban sin base de datos; los repositorios, con pruebas de integración.
- Las lecturas también abren una transacción (dos idas y vueltas más a la base). Es aceptable
  a la escala de una barbería; si una consulta caliente lo necesita, puede tener un puerto
  propio sobre el *pool*.
- Un `derive(Debug)` sobre un tipo genérico exige `C: Debug`; donde estorba (fachadas), se
  implementa `Debug` a mano.
