// Concurrencia contra el servidor real (RNF-INT-01, RN-19): peticiones simultáneas por la red.
// Usa un día de la agenda que nada más ocupa (dentro de 8 días) y los barberos de la corrida.
import { assert, esperar, omitir } from '../lib/registro.mjs';

export const nombre = '12 · Concurrencia';

const horasDe = r => ['manana', 'tarde', 'noche'].flatMap(f => r[f] ?? []).map(h => h.inicio ?? h);
const conteo = respuestas => respuestas.reduce((m, r) => ({ ...m, [r.estado]: (m[r.estado] ?? 0) + 1 }), {});
const percentil = (ordenados, p) => ordenados[Math.min(ordenados.length - 1, Math.ceil((p / 100) * ordenados.length) - 1)];

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;
  const dia = d.fecha(8);
  const a = hora => d.instante(dia, hora);
  const aLaVez = fabricas => Promise.all(fabricas.map(f => f()));
  const sin5xx = respuestas => assert.ok(respuestas.every(r => r.estado < 500), `hubo errores del servidor: ${JSON.stringify(conteo(respuestas))}`);

  await t.paso('8 reservas a la misma hora con el mismo barbero: una sola; las demás 409 y sin clientes huérfanos', async () => {
    const nombreBase = `Carrera ${d.corrida}`;
    const r = await aLaVez(Array.from({ length: 8 }, (_, i) => () => admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `${nombreBase} ${i}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b2, inicio: a('10:00') })));
    assert.deepEqual(conteo(r), { 201: 1, 409: 7 });
    assert.ok(r.filter(x => x.estado === 409).every(x => x.json.codigo === 'conflicto_horario'));
    const clientes = esperar(await admin.get(`/api/v1/clientes/buscar?q=${encodeURIComponent(nombreBase)}`), 200);
    assert.equal(clientes.length, 1, 'quedaron clientes de las reservas que perdieron');
  }, { requiere: [s.corte, b.b2] });

  await t.paso('8 reservas con «Cualquier barbero» y dos barberos libres: exactamente dos, una por barbero', async () => {
    const r = await aLaVez(Array.from({ length: 8 }, (_, i) => () => admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Cualquiera ${d.corrida} ${i}`, celular: d.celular() }, servicio_id: s.carrera, barbero_id: null, inicio: a('11:00') })));
    sin5xx(r);
    const barberos = r.filter(x => x.estado === 201).map(x => x.json.barbero.id).sort();
    assert.deepEqual(barberos, [b.b2, b.b3].sort(), `se crearon ${JSON.stringify(conteo(r))}`);
  }, { requiere: [s.carrera, b.b2, b.b3] });

  await t.paso('Dos citas movidas a la misma hora a la vez: solo una se mueve', async () => {
    const crear = hora => admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Mover ${d.corrida} ${hora}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b3, inicio: a(hora) });
    const c1 = esperar(await crear('15:00'), 201).id;
    const c2 = esperar(await crear('15:30'), 201).id;
    const r = await aLaVez([c1, c2].map(id => () => admin.post(`/api/v1/agenda/citas/${id}/reprogramar`, { inicio: a('16:00'), barbero_id: b.b3 })));
    assert.deepEqual(conteo(r), { 200: 1, 409: 1 });
  }, { requiere: [s.corte, b.b3] });

  const metodos = ctx.metodos;
  await t.paso('10 ventas a la vez: tiquetes consecutivos, sin huecos ni repetidos', async () => {
    const r = await aLaVez(Array.from({ length: 10 }, () => () => admin.post('/api/v1/ventas', { cliente: { tipo: 'ocasional' }, barbero_id: b.b2, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: metodos.efectivo, monto: 38000 }] })));
    assert.deepEqual(conteo(r), { 201: 10 });
    const tiquetes = r.map(x => x.json.numero_ticket).sort((x, y) => x - y);
    assert.deepEqual(tiquetes, Array.from({ length: 10 }, (_, i) => tiquetes[0] + i));
    ctx.ventasConcurrentes = r.map(x => x.json.id);
  }, { requiere: [metodos, b.b2] });

  await t.paso('La misma cita cobrada 5 veces a la vez: una sola venta', async () => {
    const hoyHoras = horasDe(esperar(await admin.get(`/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b2}&fecha=${d.hoy()}`), 200));
    if (!hoyHoras.length) omitir('no quedan turnos hoy');
    const cita = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Cobro doble ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b2, inicio: hoyHoras[0] }), 201).id;
    const r = await aLaVez(Array.from({ length: 5 }, () => () => admin.post('/api/v1/ventas', { cita_id: cita, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: metodos.efectivo, monto: 38000 }] })));
    assert.deepEqual(conteo(r), { 201: 1, 409: 4 });
    assert.ok(r.filter(x => x.estado === 409).every(x => x.json.codigo === 'cita_ya_cobrada'));
  }, { requiere: [metodos, b.b2] });

  await t.paso('La misma venta anulada 5 veces a la vez: una anulación', async () => {
    const venta = ctx.ventasConcurrentes[0];
    const r = await aLaVez(Array.from({ length: 5 }, () => () => admin.post(`/api/v1/ventas/${venta}/anulacion`, { motivo: 'Anulación concurrente' })));
    assert.deepEqual(conteo(r), { 200: 1, 409: 4 });
  }, { requiere: [ctx.ventasConcurrentes] });

  await t.paso('El mismo celular registrado 5 veces a la vez: un cliente, cuatro 409', async () => {
    const celular = d.celular();
    const r = await aLaVez(Array.from({ length: 5 }, (_, i) => () => admin.post('/api/v1/clientes', { nombre: `Mismo celular ${d.corrida} ${i}`, celular })));
    assert.deepEqual(conteo(r), { 201: 1, 409: 4 });
    assert.ok(r.filter(x => x.estado === 409).every(x => x.json.codigo === 'celular_en_uso'));
    ctx.clienteConcurrente = r.find(x => x.estado === 201).json.id;
  });

  await t.paso('10 ventas a la vez al mismo cliente: su ficha suma exactamente 10 visitas', async () => {
    const id = ctx.clienteConcurrente;
    const r = await aLaVez(Array.from({ length: 10 }, () => () => admin.post('/api/v1/ventas', { cliente: { tipo: 'registrado', id }, barbero_id: b.b3, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: metodos.efectivo, monto: 38000 }] })));
    assert.deepEqual(conteo(r), { 201: 10 });
    const f = esperar(await admin.get(`/api/v1/clientes/${id}`), 200);
    assert.deepEqual([f.visitas, f.total_gastado], [10, 380000]);
  }, { requiere: [ctx.clienteConcurrente, metodos, b.b3] });

  await t.paso('Transiciones cruzadas de una cita (cancelar, confirmar, iniciar): sin errores y un estado coherente', async () => {
    const cita = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Cruce ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b2, inicio: a('17:00') }), 201).id;
    const r = await aLaVez([
      () => admin.post(`/api/v1/agenda/citas/${cita}/cancelar`, { motivo: 'solicitud_cliente', avisar_cliente: false }),
      () => admin.post(`/api/v1/agenda/citas/${cita}/confirmar`),
      () => admin.post(`/api/v1/agenda/citas/${cita}/confirmar`),
      () => admin.post(`/api/v1/agenda/citas/${cita}/recordatorio`),
    ]);
    sin5xx(r);
    const detalle = esperar(await admin.get(`/api/v1/agenda/citas/${cita}`), 200);
    assert.ok(['cancelada', 'confirmada'].includes(detalle.estado), detalle.estado);
    const eventos = detalle.historial.map(e => e.evento);
    assert.equal(eventos.filter(e => e === 'cancelada').length, detalle.estado === 'cancelada' ? 1 : 0);
    if (detalle.estado === 'cancelada') assert.equal(eventos.at(-1), 'cancelada');
  }, { requiere: [s.corte, b.b2] });

  await t.paso('Reserva en línea: 3 clientes verificados toman la misma hora; una gana y los comprobantes de las otras siguen sirviendo', async () => {
    if (!ctx.servidor.disponible) omitir('sin DIAMANTE_SSH');
    const comprobantes = [];
    for (let i = 0; i < 3; i++) {
      const celular = d.celular();
      const pedido = await ctx.anonimo.post('/api/v1/publico/verificaciones', { celular }, { reintentar429: false });
      if (pedido.estado === 429) omitir(`límite de 10 códigos de verificación por IP y hora (reintente en ${pedido.cabeceras.get('retry-after')} s)`);
      const v = esperar(pedido, 202);
      const codigo = await ctx.servidor.codigoDeVerificacion(celular);
      comprobantes.push(esperar(await ctx.anonimo.post(`/api/v1/publico/verificaciones/${v.id}/confirmar`, { codigo }), 200).comprobante);
    }
    const reserva = (comprobante, inicio, i) => ({ comprobante, servicio_id: s.corte, barbero_id: b.b3, inicio, cliente: { nombre: `En línea ${d.corrida} ${i}`, correo: null, fecha_nacimiento: null }, autoriza_tratamiento_datos: true, politica_version_id: ctx.politica });
    const r = await aLaVez(comprobantes.map((c, i) => () => ctx.anonimo.post('/api/v1/publico/citas', reserva(c, a('12:00'), i))));
    assert.deepEqual(conteo(r), { 201: 1, 409: 2 });
    const perdedores = comprobantes.filter((_, i) => r[i].estado === 409);
    for (const [i, c] of perdedores.entries()) {
      esperar(await ctx.anonimo.post('/api/v1/publico/citas', reserva(c, a(i ? '13:00' : '12:30'), i + 10)), 201);
    }
  }, { requiere: [s.corte, b.b3, ctx.politica] });

  const rafaga = async (nombreRafaga, peticiones) => {
    const r = await Promise.all(peticiones.map(f => f()));
    sin5xx(r);
    const tiempos = r.map(x => Math.round(x.ms)).sort((x, y) => x - y);
    ctx.metricas.push({ nombre: nombreRafaga, n: r.length, p50: percentil(tiempos, 50), p95: percentil(tiempos, 95), max: tiempos.at(-1) });
    return r;
  };

  await t.paso('Ráfaga pública: 40 consultas de disponibilidad simultáneas', async () => {
    const r = await rafaga('Disponibilidad pública ×40', Array.from({ length: 40 }, (_, i) => () => ctx.anonimo.get(`/api/v1/publico/disponibilidad/horas?servicio_id=${s.carrera}&fecha=${d.fecha(1 + (i % 7))}`)));
    assert.ok(r.every(x => x.estado === 200));
  }, { requiere: [s.carrera] });

  await t.paso('Ráfaga del panel: 60 lecturas simultáneas (agenda, clientes, caja, dashboard)', async () => {
    const rutas = [`/api/v1/agenda/citas?desde=${dia}&hasta=${dia}`, `/api/v1/clientes/buscar?q=${d.corrida}`, '/api/v1/ventas/caja', '/api/v1/reportes/dashboard', `/api/v1/agenda/tablero?fecha=${dia}`, '/api/v1/ventas/catalogos'];
    const r = await rafaga('Panel ×60', Array.from({ length: 60 }, (_, i) => () => admin.get(rutas[i % rutas.length])));
    assert.ok(r.every(x => x.estado === 200), JSON.stringify(conteo(r)));
  });
}
