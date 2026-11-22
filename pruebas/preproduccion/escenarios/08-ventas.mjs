// Ventas y caja (RF-VEN): cobro desde la cita con descuento, propina y dos métodos; cliente
// nuevo, ocasional y registrado; validaciones; anulación auditada; caja; «Lo que llevas hoy»;
// el barbero en su panel; cierre de caja y métodos de pago (se dejan como estaban).
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '08 · Ventas y caja';

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;
  const v = ctx.creado.ventas;

  const cat = await t.paso('Catálogos de la venta: métodos activos, descuentos, motivos y propinas rápidas', async () => {
    const r = esperar(await admin.get('/api/v1/ventas/catalogos'), 200);
    const metodo = codigo => r.metodos_pago.find(m => m.codigo === codigo)?.id;
    const c = { efectivo: metodo('efectivo'), nequi: metodo('nequi'), diez: r.descuentos.find(x => x.nombre === '10 %')?.id, motivo: r.motivos_descuento[0]?.id };
    assert.ok(c.efectivo && c.nequi && c.diez && c.motivo, JSON.stringify(c));
    ctx.metodos = c;
    return c;
  });

  await t.paso('Cobrar la cita en curso: corte + cejas, 10 %, propina y dos métodos de pago', async () => {
    if (!ctx.citaEnCurso) omitir('no hubo cita en curso hoy');
    const cuerpo = { cita_id: ctx.citaEnCurso.id, servicio_ids: [s.corte, s.cejas], descuento: { descuento_id: cat.diez, motivo_descuento_id: cat.motivo }, propina: 3000, pagos: [{ metodo_pago_id: cat.efectivo, monto: 20000 }, { metodo_pago_id: cat.nequi, monto: 26200 }] };
    const r = esperar(await admin.post('/api/v1/ventas', cuerpo), 201);
    assert.deepEqual([r.subtotal, r.descuento.valor, r.venta_neta, r.total_cobrado, r.comision_barbero], [48000, 4800, 43200, 46200, 19440]);
    assert.equal(r.cita_id, ctx.citaEnCurso.id);
    v.cita = r.id;
    assert.equal(esperar(await admin.get(`/api/v1/agenda/citas/${ctx.citaEnCurso.id}`), 200).estado, 'atendida');
    esperarError(await admin.post('/api/v1/ventas', cuerpo), 409, 'cita_ya_cobrada');
  }, { requiere: [cat] });

  await t.paso('Pagos que no cuadran → 422 en «pagos»', async () => {
    const r = esperar(await admin.post('/api/v1/ventas', { cliente: { tipo: 'ocasional' }, barbero_id: b.b2, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: cat.efectivo, monto: 1000 }] }), 422);
    assert.ok(r.errores.some(e => e.campo === 'pagos'));
  }, { requiere: [cat, b.b2] });

  await t.paso('Un servicio que el barbero no hace → 422', async () => {
    esperar(await admin.post('/api/v1/ventas', { cliente: { tipo: 'ocasional' }, barbero_id: b.b3, servicio_ids: [s.cejas], pagos: [{ metodo_pago_id: cat.efectivo, monto: 10000 }] }), 422);
  }, { requiere: [cat, b.b3] });

  await t.paso('Venta a un cliente nuevo (se crea) y a un cliente ocasional (no suma a nadie)', async () => {
    const nuevo = esperar(await admin.post('/api/v1/ventas', { cliente: { tipo: 'nuevo', nombre: `Valeria Ortiz ${d.corrida}`, celular: d.celular(), correo: null }, barbero_id: b.b2, servicio_ids: [s.corte], propina: 2000, pagos: [{ metodo_pago_id: cat.nequi, monto: 40000 }] }), 201);
    assert.ok(nuevo.cliente.id);
    v.nuevo = nuevo.id;
    const ocasional = esperar(await admin.post('/api/v1/ventas', { cliente: { tipo: 'ocasional' }, barbero_id: b.b2, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: cat.efectivo, monto: 38000 }] }), 201);
    assert.equal(ocasional.cliente, null);
    v.ocasional = ocasional.id;
    assert.equal(ocasional.numero_ticket, nuevo.numero_ticket + 1, 'los tiquetes son consecutivos');
  }, { requiere: [cat, b.b2] });

  await t.paso('Venta a un cliente registrado: su ficha suma visita y gasto', async () => {
    const diana = ctx.creado.clientes.diana;
    if (!diana) omitir('sin la clienta del escenario 05');
    const antes = esperar(await admin.get(`/api/v1/clientes/${diana.id}`), 200);
    esperar(await admin.post('/api/v1/ventas', { cliente: { tipo: 'registrado', id: diana.id }, barbero_id: b.b1, servicio_ids: [s.cejas], pagos: [{ metodo_pago_id: cat.efectivo, monto: 10000 }] }), 201);
    const despues = esperar(await admin.get(`/api/v1/clientes/${diana.id}`), 200);
    assert.deepEqual([despues.visitas, despues.total_gastado], [antes.visitas + 1, antes.total_gastado + 10000]);
    assert.equal(despues.ultima_visita, d.hoy());
  }, { requiere: [cat, b.b1] });

  await t.paso('Listar ventas (hoy, por barbero, por cliente) y ver el detalle', async () => {
    const hoy = esperar(await admin.get(`/api/v1/ventas?fecha=${d.hoy()}&por_pagina=100`), 200);
    const ids = (hoy.datos ?? hoy).map(x => x.id);
    assert.ok(ids.includes(v.ocasional));
    const porBarbero = esperar(await admin.get(`/api/v1/ventas?fecha=${d.hoy()}&barbero_id=${b.b2}`), 200);
    assert.ok((porBarbero.datos ?? porBarbero).every(x => x.barbero.id === b.b2));
    esperar(await admin.get(`/api/v1/ventas/${v.ocasional}`), 200);
  }, { requiere: [v.ocasional] });

  await t.paso('Anular una venta (auditada); anularla otra vez → 409', async () => {
    const r = esperar(await admin.post(`/api/v1/ventas/${v.ocasional}/anulacion`, { motivo: 'Se registró dos veces por error' }), 200);
    assert.equal(r.estado, 'anulada');
    esperarError(await admin.post(`/api/v1/ventas/${v.ocasional}/anulacion`, { motivo: 'Otra vez por error' }), 409, 'estado_invalido');
    const auditoria = esperar(await admin.get(`/api/v1/auditoria?entidad=venta&entidad_id=${v.ocasional}`), 200);
    assert.ok(auditoria.datos.some(x => x.accion === 'anular_venta'));
  }, { requiere: [v.ocasional] });

  await t.paso('Caja del día: cuadra (neta + propinas) y no cuenta las anuladas', async () => {
    const c = esperar(await admin.get('/api/v1/ventas/caja'), 200);
    assert.equal(c.total_cobrado, c.venta_neta + c.propinas);
    assert.equal(c.por_metodo.reduce((s2, m) => s2 + m.total, 0), c.total_cobrado);
    assert.ok(!c.ventas.some(x => x.id === v.ocasional && x.estado === 'registrada'));
  });

  await t.paso('«Lo que llevas hoy» de un barbero (vista de la administradora)', async () => {
    const r = esperar(await admin.get(`/api/v1/ventas/resumen-barbero?barbero_id=${b.b2}`), 200);
    assert.equal(r.total_a_recibir, r.comision + r.propinas);
  }, { requiere: [b.b2] });

  await t.paso('El barbero en su panel: vende a su nombre, ve solo lo suyo y no anula', async () => {
    if (!ctx.barbero) omitir('sin sesión de barbero (escenario 04)');
    const br = ctx.barbero;
    const catalogos = esperar(await br.get('/api/v1/ventas/catalogos'), 200);
    assert.ok(catalogos.descuentos.every(x => x.permitido_a_barberos !== false));
    const propia = esperar(await br.post('/api/v1/ventas', { cliente: { tipo: 'ocasional' }, barbero_id: b.b2, servicio_ids: [s.corte], propina: 5000, pagos: [{ metodo_pago_id: cat.efectivo, monto: 43000 }] }), 201);
    assert.equal(propia.barbero.id, b.b1, 'la venta del barbero queda a su nombre');
    esperar(await br.post(`/api/v1/ventas/${propia.id}/anulacion`, { motivo: 'Me equivoqué' }), 403);
    esperar(await br.get(`/api/v1/ventas/${v.nuevo}`), 404);
    const mio = esperar(await br.get('/api/v1/ventas/resumen-barbero'), 200);
    assert.ok(mio.numero_ventas >= 1);
    esperar(await br.get('/api/v1/ventas/caja'), 403);
    const mias = esperar(await br.get(`/api/v1/ventas?fecha=${d.hoy()}`), 200);
    assert.ok((mias.datos ?? mias).every(x => x.barbero.id === b.b1));
  }, { requiere: [cat, b.b1] });

  await t.paso('Cierre de caja del día (uno por día: si ya se cerró hoy, 409)', async () => {
    const r = await admin.post('/api/v1/ventas/cierres', { fecha: d.hoy(), contado: [{ metodo_pago_id: cat.efectivo, monto: 100000 }], observacion: `Conteo ${d.sufijo}` });
    if (r.estado === 409) assert.equal(r.json.codigo, 'cierre_existente');
    else esperar(r, 201);
    esperar(await admin.get(`/api/v1/ventas/cierres?fecha=${d.hoy()}`), 200);
  }, { requiere: [cat] });

  await t.paso('Métodos de pago: desactivar uno lo quita del formulario; se deja como estaba', async () => {
    const metodos = esperar(await admin.get('/api/v1/ventas/metodos-pago'), 200);
    const daviplata = metodos.find(m => m.codigo === 'daviplata');
    if (!daviplata?.activo) omitir('Daviplata no está activo: no se toca');
    esperar(await admin.patch(`/api/v1/ventas/metodos-pago/${daviplata.id}`, { activo: false }), 200);
    try {
      const c = esperar(await admin.get('/api/v1/ventas/catalogos'), 200);
      assert.ok(!c.metodos_pago.some(m => m.id === daviplata.id));
    } finally {
      esperar(await admin.patch(`/api/v1/ventas/metodos-pago/${daviplata.id}`, { activo: true }), 200);
    }
  });

  await t.paso('Venta inexistente → 404', async () => {
    esperar(await admin.get('/api/v1/ventas/999999999'), 404);
  });
}
