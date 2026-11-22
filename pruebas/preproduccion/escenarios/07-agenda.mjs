// Agenda del panel (RF-AGE): crear, listar, tablero, confirmar, recordatorio, reprogramar,
// reasignar, pedir reprogramación, bloqueo con citas (aviso), cancelar, iniciar, inasistencia,
// pendientes de cierre y configuración (que se guarda sin cambios).
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '07 · Agenda del panel';

const horasDe = r => ['manana', 'tarde', 'noche'].flatMap(f => r[f] ?? []).map(h => h.inicio ?? h);

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;
  const dia = d.fecha(5);

  const horas = await t.paso('Horas libres para el personal', async () => {
    const h = horasDe(esperar(await admin.get(`/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b3}&fecha=${dia}`), 200));
    assert.ok(h.length >= 40);
    return h;
  }, { requiere: [s.corte, b.b3] });

  const celular = d.celular();
  const cita = await t.paso('Crear una cita por teléfono (cliente nuevo): sin confirmar, con su confirmación por WhatsApp en cola', async () => {
    const r = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Carlos Pérez ${d.corrida}`, celular }, servicio_id: s.corte, barbero_id: b.b3, inicio: horas[20], nota: 'Llamó por teléfono.' }), 201);
    assert.equal(r.estado, 'sin_confirmar');
    assert.equal(r.historial[0].evento, 'creada');
    ctx.celularAgenda = celular;
    ctx.creado.clientes.carlos = { id: r.cliente.id, celular };
    return r.id;
  }, { requiere: [horas] });

  await t.paso('La misma hora con el mismo barbero → 409 conflicto_horario con alternativas', async () => {
    const r = esperarError(await admin.post('/api/v1/agenda/citas', { cliente_id: ctx.creado.clientes.diana?.id ?? ctx.creado.clientes.carlos.id, servicio_id: s.corte, barbero_id: b.b3, inicio: horas[20] }), 409, 'conflicto_horario');
    assert.ok(Array.isArray(r.alternativas) && r.alternativas.length > 0);
  }, { requiere: [cita] });

  await t.paso('Listar por fecha y barbero, ver el detalle y el tablero', async () => {
    const lista = esperar(await admin.get(`/api/v1/agenda/citas?desde=${dia}&hasta=${dia}&barbero_id=${b.b3}`), 200);
    assert.ok(lista.some(x => x.id === cita));
    const detalle = esperar(await admin.get(`/api/v1/agenda/citas/${cita}`), 200);
    assert.ok(detalle.mensajes.some(m => m.plantilla === 'cita_confirmada'), 'sin mensaje de confirmación');
    esperar(await admin.get(`/api/v1/agenda/tablero?fecha=${dia}`), 200);
  }, { requiere: [cita] });

  await t.paso('Confirmar y enviar un recordatorio manual (el segundo seguido se frena)', async () => {
    assert.equal(esperar(await admin.post(`/api/v1/agenda/citas/${cita}/confirmar`), 200).estado, 'confirmada');
    esperar(await admin.post(`/api/v1/agenda/citas/${cita}/recordatorio`), [200, 202]);
    const otra = await admin.post(`/api/v1/agenda/citas/${cita}/recordatorio`);
    assert.ok([409, 429].includes(otra.estado), `segundo recordatorio: ${otra.estado}`);
  }, { requiere: [cita] });

  await t.paso('Reprogramar a otra hora con otro barbero', async () => {
    const r = esperar(await admin.post(`/api/v1/agenda/citas/${cita}/reprogramar`, { inicio: horas[24], barbero_id: b.b2 }), 200);
    assert.equal(r.barbero.id, b.b2);
    assert.ok(['reprogramada', 'reasignada'].includes(r.historial.at(-1).evento), r.historial.at(-1).evento);
  }, { requiere: [cita] });

  await t.paso('Reasignar en lote y pedir al cliente que elija otra hora', async () => {
    const r = esperar(await admin.post('/api/v1/agenda/citas/reasignacion', { cita_ids: [cita], barbero_destino_id: b.b3 }), 200);
    assert.equal(r[0].barbero.id, b.b3);
    assert.equal(esperar(await admin.post('/api/v1/agenda/citas/aviso-reprogramacion', { cita_ids: [cita] }), 200).avisadas, 1);
  }, { requiere: [cita] });

  await t.paso('Un bloqueo que deja una cita por fuera genera un aviso para la administradora', async () => {
    const detalle = esperar(await admin.get(`/api/v1/agenda/citas/${cita}`), 200);
    const hora = detalle.inicio.slice(11, 16);
    const fin = detalle.fin.slice(11, 16);
    const r = esperar(await admin.post(`/api/v1/barberos/${b.b3}/bloqueos`, { fecha: dia, desde: hora, hasta: fin, motivo: 'otro', nota: `Bloqueo ${d.sufijo}` }), 201);
    ctx.citaConAviso = cita;
    esperar(await admin.delete(`/api/v1/barberos/${b.b3}/bloqueos/${r.id}`), 204);
  }, { requiere: [cita] });

  await t.paso('Cancelar con motivo (y un motivo inválido → 422)', async () => {
    esperar(await admin.post(`/api/v1/agenda/citas/${cita}/cancelar`, { motivo: 'porque si', avisar_cliente: true }), 422);
    const r = esperar(await admin.post(`/api/v1/agenda/citas/${cita}/cancelar`, { motivo: 'barbero_no_disponible', avisar_cliente: true }), 200);
    assert.equal(r.estado, 'cancelada');
    esperarError(await admin.post(`/api/v1/agenda/citas/${cita}/confirmar`), 409);
  }, { requiere: [cita] });

  await t.paso('No se anonimiza a un cliente con citas por atender (409)', async () => {
    const diana = ctx.creado.clientes.diana;
    if (!diana) omitir('sin la clienta del escenario 05');
    const c = esperar(await admin.post('/api/v1/agenda/citas', { cliente_id: diana.id, servicio_id: s.corte, barbero_id: b.b3, inicio: horas[30] }), 201);
    esperar(await admin.post(`/api/v1/clientes/${diana.id}/anonimizacion`, { motivo: 'Prueba' }), 409);
    esperar(await admin.post(`/api/v1/agenda/citas/${c.id}/cancelar`, { motivo: 'otro', avisar_cliente: false }), 200);
  }, { requiere: [horas] });

  await t.paso('Hoy: iniciar una cita del turno actual (queda en curso para cobrarla)', async () => {
    const hoyHoras = horasDe(esperar(await admin.get(`/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b1}&fecha=${d.hoy()}`), 200));
    if (!hoyHoras.length) omitir('no quedan turnos hoy');
    const c = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Pablo Vélez ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b1, inicio: hoyHoras[0] }), 201);
    assert.equal(esperar(await admin.post(`/api/v1/agenda/citas/${c.id}/iniciar`), 200).estado, 'en_curso');
    ctx.citaEnCurso = { id: c.id, cliente: c.cliente.id };
  }, { requiere: [s.corte, b.b1] });

  await t.paso('Hoy: marcar inasistencia de una cita cuyo turno ya empezó (suma en la ficha)', async () => {
    const hoyHoras = horasDe(esperar(await admin.get(`/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b3}&fecha=${d.hoy()}`), 200));
    const actual = hoyHoras.find(h => Date.parse(h) <= Date.now());
    if (!actual) omitir('el turno actual no ha empezado o no quedan turnos hoy');
    const c = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Marta Gil ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b3, inicio: actual }), 201);
    assert.equal(esperar(await admin.post(`/api/v1/agenda/citas/${c.id}/inasistencia`), 200).estado, 'no_asistio');
    assert.equal(esperar(await admin.get(`/api/v1/clientes/${c.cliente.id}`), 200).inasistencias, 1);
  }, { requiere: [s.corte, b.b3] });

  await t.paso('Pendientes de cierre del día', async () => {
    assert.ok(Array.isArray(esperar(await admin.get(`/api/v1/agenda/pendientes-de-cierre?fecha=${d.hoy()}`), 200)));
  });

  await t.paso('Configuración de la agenda: leer, guardar igual y rechazar valores fuera de las opciones', async () => {
    const c = esperar(await admin.get('/api/v1/agenda/configuracion'), 200);
    esperar(await admin.put('/api/v1/agenda/configuracion', c), 200);
    esperar(await admin.put('/api/v1/agenda/configuracion', { ...c, intervalo_min: 45 }), 422);
  });

  await t.paso('Cita inexistente → 404', async () => {
    esperar(await admin.get('/api/v1/agenda/citas/999999999'), 404);
  });
}
