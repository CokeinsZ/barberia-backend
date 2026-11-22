// Notificaciones (RF-NOT): registro de mensajes (sin credenciales de Meta quedan en cola, sin
// exponer sus variables), avisos internos y webhook de WhatsApp sin configurar.
import { assert, esperar } from '../lib/registro.mjs';

export const nombre = '10 · Mensajes, avisos y webhook';

export default async function (ctx, t) {
  const admin = ctx.admin;

  await t.paso('Mensajes de un cliente: confirmación y avisos en la cola, sin sus variables', async () => {
    const r = esperar(await admin.get(`/api/v1/mensajes?destinatario=${ctx.celularAgenda}`), 200);
    const lista = r.datos ?? r;
    assert.ok(lista.some(m => m.plantilla === 'cita_confirmada'), 'sin confirmación');
    assert.ok(lista.every(m => !('variables' in m)), 'el registro no debe mostrar las variables (códigos, enlaces)');
  }, { requiere: [ctx.celularAgenda] });

  await t.paso('Aviso por el bloqueo con citas: aparece sin leer y se marca leído (idempotente)', async () => {
    const r = esperar(await admin.get('/api/v1/avisos?no_leidos=true&por_pagina=100'), 200);
    const aviso = r.datos.find(a => (a.datos?.cita_ids ?? []).includes(ctx.citaConAviso));
    assert.ok(aviso, 'no apareció el aviso de la cita afectada');
    assert.equal(aviso.tipo, 'bloqueo_con_citas');
    esperar(await admin.post(`/api/v1/avisos/${aviso.id}/leido`), [200, 204]);
    esperar(await admin.post(`/api/v1/avisos/${aviso.id}/leido`), [200, 204]);
    const despues = esperar(await admin.get('/api/v1/avisos?no_leidos=true&por_pagina=100'), 200);
    assert.ok(!despues.datos.some(a => a.id === aviso.id));
  }, { requiere: [ctx.citaConAviso] });

  await t.paso('Webhook de WhatsApp sin configurar: no acepta verificaciones ni eventos', async () => {
    const g = await ctx.anonimo.get('/api/v1/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=adivinado&hub.challenge=123');
    assert.ok([403, 404].includes(g.estado), `verificación: ${g.estado}`);
    const p = await ctx.anonimo.post('/api/v1/webhooks/whatsapp', { entry: [] });
    assert.ok([401, 404].includes(p.estado), `evento sin firma: ${p.estado}`);
  });
}
