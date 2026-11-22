// Límites de uso (va al final: deja la IP con el límite público agotado un minuto).
import { assert } from '../lib/registro.mjs';

export const nombre = '13 · Límites de uso';

export default async function (ctx, t) {
  await t.paso('Límite público: 120 por minuto por IP; después 429 con Retry-After (la salud no se limita)', async () => {
    const r = await Promise.all(Array.from({ length: 130 }, () => ctx.anonimo.get('/api/v1/publico/servicios', { reintentar429: false })));
    const limitadas = r.filter(x => x.estado === 429);
    assert.ok(limitadas.length >= 10, `solo ${limitadas.length} respuestas 429`);
    assert.ok(limitadas.every(x => Number(x.cabeceras.get('retry-after')) > 0));
    assert.equal(limitadas[0].json.codigo, 'demasiados_intentos');
    assert.equal((await ctx.anonimo.get('/api/salud', { reintentar429: false })).estado, 200);
  });

  await t.paso('Un X-Forwarded-For inventado no cambia la IP con que se cuenta', async () => {
    const r = await ctx.anonimo.get('/api/v1/publico/servicios', { reintentar429: false, cabeceras: { 'X-Forwarded-For': '203.0.113.77' } });
    assert.equal(r.estado, 429);
  });
}
