// Limpieza (siempre corre): cancela las citas pendientes de los barberos de la corrida, los
// retira (salen de la landing), desactiva sus servicios y cierra las sesiones. Las ventas y
// clientes quedan (son historia); la base de preproducción se reinicia antes de producción.
import { assert, esperar } from '../lib/registro.mjs';

export const nombre = '99 · Limpieza';

export default async function (ctx, t) {
  const admin = ctx.admin;
  if (!admin) return;
  const { d } = ctx;
  const b = ctx.creado.barberos;
  const ids = Object.values(b).filter(Boolean);

  await t.paso('Cancelar las citas pendientes de los barberos de la corrida', async () => {
    let canceladas = 0;
    for (const id of ids) {
      const citas = esperar(await admin.get(`/api/v1/agenda/citas?desde=${d.hoy()}&hasta=${d.fecha(40)}&barbero_id=${id}`), 200);
      for (const c of citas.filter(x => ['sin_confirmar', 'confirmada', 'en_curso'].includes(x.estado))) {
        const r = await admin.post(`/api/v1/agenda/citas/${c.id}/cancelar`, { motivo: 'otro', avisar_cliente: false });
        if (r.estado === 200) canceladas++;
      }
    }
    return canceladas;
  }, { requiere: [ids.length] });

  await t.paso('Retirar a los barberos de la corrida (salen de la landing)', async () => {
    for (const id of ids) esperar(await admin.post(`/api/v1/barberos/${id}/retiro`, { fecha: d.hoy(), motivo: 'otro' }), [200, 409]);
    const landing = esperar(await ctx.anonimo.get('/api/v1/publico/landing'), [200, 429]);
    if (landing) assert.ok(!landing.barberos.some(x => ids.includes(x.id)), 'un barbero retirado sigue en la landing');
  }, { requiere: [ids.length] });

  await t.paso('Desactivar los servicios de la corrida', async () => {
    for (const id of Object.values(ctx.creado.servicios)) esperar(await admin.patch(`/api/v1/servicios/${id}/estado`, { activo: false }), 200);
  }, { requiere: [Object.keys(ctx.creado.servicios).length] });

  await t.paso('Cerrar las sesiones: después ya no sirven', async () => {
    for (const cliente of [ctx.barbero, admin].filter(Boolean)) {
      esperar(await cliente.delete('/api/v1/auth/sesion'), 204);
      esperar(await cliente.get('/api/v1/auth/sesion'), 401);
    }
  });
}
