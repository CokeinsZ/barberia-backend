// Reportes y dashboard (RF-REP, RF-DAS, RF-PBA-06). Las cifras se comprueban por coherencia (la
// base tiene más datos que los de esta corrida) y contra la caja; los PDF los genera Gotenberg.
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '09 · Reportes, dashboard y PDF';

const suma = (lista, campo) => lista.reduce((s, x) => s + x[campo], 0);

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const b = ctx.creado.barberos;
  const hoy = d.hoy();

  const resumen = await t.paso('Resumen del mes: por barbero, método, semana y servicio cuadran con los totales', async () => {
    const r = esperar(await admin.get('/api/v1/reportes/resumen'), 200);
    const x = r.totales;
    assert.equal(r.periodo.hasta, hoy);
    assert.equal(x.total_cobrado, x.ventas_netas + x.propinas);
    assert.equal(x.ventas_netas, x.subtotal - x.descuentos);
    assert.equal(suma(r.por_barbero, 'ventas_netas'), x.ventas_netas);
    assert.equal(suma(r.por_barbero, 'comision'), x.comisiones);
    assert.equal(suma(r.por_metodo, 'total'), x.total_cobrado);
    assert.equal(suma(r.por_semana, 'ventas_netas'), x.ventas_netas);
    assert.equal(suma(r.por_servicio, 'unidades'), x.servicios);
    r.por_barbero.forEach(f => assert.equal(f.total_a_pagar, f.comision + f.propinas));
    return r;
  });

  const deHoy = await t.paso('Resumen de hoy = caja del día', async () => {
    const r = esperar(await admin.get(`/api/v1/reportes/resumen?desde=${hoy}&hasta=${hoy}`), 200);
    const c = esperar(await admin.get('/api/v1/ventas/caja'), 200);
    assert.deepEqual([c.numero_ventas, c.venta_neta, c.propinas, c.total_cobrado], [r.totales.clientes_atendidos, r.totales.ventas_netas, r.totales.propinas, r.totales.total_cobrado]);
    return r.totales;
  });

  await t.paso('Dashboard de hoy: coincide con el resumen; tendencia de 12 días; citas por estado', async () => {
    const r = esperar(await admin.get('/api/v1/reportes/dashboard'), 200);
    assert.equal(r.fecha, hoy);
    if (deHoy) assert.deepEqual(r.hoy, deHoy);
    assert.equal(suma(r.barberos, 'ventas_netas'), r.hoy.ventas_netas);
    assert.equal(r.tendencia.length, 12);
    assert.deepEqual(r.tendencia.at(-1), { fecha: hoy, ventas_netas: r.hoy.ventas_netas });
    r.barberos.forEach(x => { if (x.ocupacion !== null) assert.ok(x.ocupacion >= 0 && x.ocupacion <= 100); });
    esperar(await admin.get(`/api/v1/reportes/dashboard?fecha=${d.fecha(-1)}`), 200);
    esperarError(await admin.get(`/api/v1/reportes/dashboard?fecha=${d.fecha(1)}`), 422);
  });

  await t.paso('Tendencias y alertas', async () => {
    const tr = esperar(await admin.get('/api/v1/reportes/tendencias'), 200);
    assert.deepEqual(tr.clientes_por_mes.map(m => m.mes), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    if (resumen) assert.deepEqual(tr.por_servicio, resumen.por_servicio);
    const al = esperar(await admin.get('/api/v1/reportes/alertas'), 200);
    for (const k of ['sin_confirmar_manana', 'cumpleanos', 'ausentes', 'por_reactivar', 'solicitudes_titular', 'avisos_no_leidos']) assert.ok(k in al, `falta ${k}`);
    const avisos = esperar(await admin.get('/api/v1/avisos?no_leidos=true&por_pagina=1'), 200);
    assert.equal(avisos.total, al.avisos_no_leidos);
  });

  await t.paso('Reporte de clientes', async () => {
    const r = esperar(await admin.get('/api/v1/reportes/clientes'), 200);
    assert.equal(r.clientes.recurrentes, r.clientes.atendidos - r.clientes.nuevos);
    assert.ok(r.por_reactivar.clientes.length <= Math.min(100, r.por_reactivar.total));
  });

  await t.paso('Liquidación de un barbero: la comisión y las propinas suman las de cada venta', async () => {
    const r = esperar(await admin.get(`/api/v1/reportes/barberos/${b.b1}`), 200);
    const l = r.liquidacion;
    assert.equal(l.comision, suma(r.ventas, 'comision'));
    assert.equal(l.propinas, suma(r.ventas, 'propina'));
    assert.equal(l.ventas_netas, suma(r.ventas, 'ventas_netas'));
    assert.equal(l.total_a_pagar, l.comision + l.propinas);
    if (resumen) {
      const fila = resumen.por_barbero.find(f => f.barbero_id === b.b1);
      if (fila) assert.deepEqual(fila, l);
    }
  }, { requiere: [b.b1] });

  const antesAuditoria = await t.paso('Auditoría de exportaciones (antes)', async () => esperar(await admin.get('/api/v1/auditoria?entidad=reporte'), 200).total);

  const pdf = async (cliente, ruta, nombreArchivo) => {
    const r = await cliente.get(ruta);
    esperar(r, 200, ruta);
    assert.equal(r.cabeceras.get('content-type'), 'application/pdf');
    assert.equal(r.bytes.subarray(0, 4).toString(), '%PDF');
    assert.match(r.cabeceras.get('content-disposition') ?? '', nombreArchivo);
    assert.equal(r.cabeceras.get('cache-control'), 'private, no-store');
    return r;
  };
  let pdfs = 0;
  for (const [nombrePdf, ruta, archivo] of [
    ['Resumen del mes', '/api/v1/reportes/resumen.pdf', /filename="resumen-\d{4}-\d{2}\.pdf"/],
    ['Clientes del mes', '/api/v1/reportes/clientes.pdf', /filename="clientes-/],
    ['Todos los barberos', '/api/v1/reportes/barberos.pdf', /filename="barberos-/],
    ['Un barbero', `/api/v1/reportes/barberos/${b.b1}.pdf`, new RegExp(`filename="barbero-${b.b1}-`)],
  ]) {
    await t.paso(`PDF: ${nombrePdf} (Gotenberg)`, async () => { await pdf(admin, ruta, archivo); pdfs++; }, { requiere: [b.b1] });
  }

  await t.paso('Cada PDF quedó en la auditoría (exportar_reporte)', async () => {
    const r = esperar(await admin.get('/api/v1/auditoria?entidad=reporte'), 200);
    assert.equal(r.total, antesAuditoria + pdfs);
    assert.equal(r.datos[0].accion, 'exportar_reporte');
  }, { requiere: [antesAuditoria !== undefined] });

  await t.paso('Periodos inválidos → 422; parámetros desconocidos → 400', async () => {
    esperarError(await admin.get('/api/v1/reportes/resumen?mes=2099-01'), 422);
    esperarError(await admin.get(`/api/v1/reportes/resumen?desde=${hoy}&hasta=2099-12-31`), 422);
    esperar(await admin.get('/api/v1/reportes/resumen?formato=xlsx'), 400);
  });

  await t.paso('«Mi reporte» del barbero: el suyo sí (y en PDF); el de otro y los generales no', async () => {
    if (!ctx.barbero) omitir('sin sesión de barbero (escenario 04)');
    const br = ctx.barbero;
    const propio = esperar(await br.get(`/api/v1/reportes/barberos/${b.b1}`), 200);
    assert.equal(propio.barbero.id, b.b1);
    await pdf(br, `/api/v1/reportes/barberos/${b.b1}.pdf`, /filename="barbero-/);
    esperar(await br.get(`/api/v1/reportes/barberos/${b.b2}`), 404);
    for (const ruta of ['/api/v1/reportes/resumen', '/api/v1/reportes/dashboard', '/api/v1/reportes/barberos.pdf']) esperar(await br.get(ruta), 403);
  }, { requiere: [b.b1, b.b2] });
}
