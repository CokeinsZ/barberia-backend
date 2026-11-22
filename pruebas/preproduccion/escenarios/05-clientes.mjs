// Clientes (RF-CLI): registro, búsqueda, ficha, edición, derechos del titular y anonimización.
import { assert, esperar, esperarError } from '../lib/registro.mjs';

export const nombre = '05 · Clientes y derechos del titular';

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const c = ctx.creado.clientes;

  const celular = d.celular();
  await t.paso('Registrar un cliente desde el panel', async () => {
    const r = esperar(await admin.post('/api/v1/clientes', { nombre: `Diana Ruiz ${d.corrida}`, celular, correo: `diana.${d.corrida}@example.com`, fecha_nacimiento: '1992-11-25' }), 201);
    c.diana = { id: r.id, celular };
  });

  await t.paso('Celular repetido → 409 celular_en_uso', async () => {
    esperarError(await admin.post('/api/v1/clientes', { nombre: 'Otra Diana', celular }), 409, 'celular_en_uso');
  }, { requiere: [c.diana] });

  await t.paso('Datos inválidos → 422', async () => {
    esperar(await admin.post('/api/v1/clientes', { nombre: 'D', celular: '123' }), 422);
  });

  await t.paso('Búsqueda rápida y listado (sin tildes ni mayúsculas)', async () => {
    const rapida = esperar(await admin.get(`/api/v1/clientes/buscar?q=${encodeURIComponent(`diana ruiz ${d.corrida}`)}`), 200);
    assert.ok(rapida.some(x => x.id === c.diana.id));
    const porCelular = esperar(await admin.get(`/api/v1/clientes/buscar?q=${celular}`), 200);
    assert.ok(porCelular.some(x => x.id === c.diana.id));
    const listado = esperar(await admin.get(`/api/v1/clientes?q=${encodeURIComponent(d.corrida)}&pagina=1`), 200);
    assert.ok(listado.datos.some(x => x.id === c.diana.id));
  }, { requiere: [c.diana] });

  await t.paso('Ficha y edición (preferencias)', async () => {
    const f = esperar(await admin.get(`/api/v1/clientes/${c.diana.id}`), 200);
    assert.equal(f.visitas, 0);
    const r = esperar(await admin.put(`/api/v1/clientes/${c.diana.id}`, { nombre: `Diana Ruiz ${d.corrida}`, celular, correo: `diana.${d.corrida}@example.com`, fecha_nacimiento: '1992-11-25', preferencias: 'Prefiere tijera.' }), 200);
    assert.equal(r.preferencias, 'Prefiere tijera.');
  }, { requiere: [c.diana] });

  await t.paso('Solicitud del titular: registrar, listar abiertas y responder (una sola vez)', async () => {
    const s = esperar(await admin.post('/api/v1/clientes/solicitudes-titular', { cliente_id: c.diana.id, tipo: 'consulta', descripcion: `Consulta ${d.sufijo}` }), 201);
    assert.ok(s.responder_antes_de, 'sin plazo legal');
    const abiertas = esperar(await admin.get('/api/v1/clientes/solicitudes-titular?abiertas=true'), 200);
    assert.ok(abiertas.datos.some(x => x.id === s.id));
    esperar(await admin.patch(`/api/v1/clientes/solicitudes-titular/${s.id}`, { respuesta: 'Se le enviaron sus datos.' }), 200);
    esperar(await admin.patch(`/api/v1/clientes/solicitudes-titular/${s.id}`, { respuesta: 'Otra respuesta' }), 409);
  }, { requiere: [c.diana] });

  await t.paso('Anonimización: el cliente desaparece de búsquedas y fichas', async () => {
    const otro = d.celular();
    const r = esperar(await admin.post('/api/v1/clientes', { nombre: `Anónimo Futuro ${d.corrida}`, celular: otro }), 201);
    esperar(await admin.post(`/api/v1/clientes/${r.id}/anonimizacion`, { motivo: 'Solicitud de supresión del titular' }), 204);
    esperar(await admin.get(`/api/v1/clientes/${r.id}`), 404);
    const busqueda = esperar(await admin.get(`/api/v1/clientes/buscar?q=${otro}`), 200);
    assert.equal(busqueda.length, 0);
    // El celular queda libre para un registro nuevo.
    esperar(await admin.post('/api/v1/clientes', { nombre: `Reutiliza ${d.corrida}`, celular: otro }), 201);
  });

  await t.paso('Cliente inexistente → 404', async () => {
    esperar(await admin.get('/api/v1/clientes/999999999'), 404);
  });
}
