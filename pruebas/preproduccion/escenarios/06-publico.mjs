// Sitio público y agendamiento en línea (RF-PUB, RF-RES): landing, disponibilidad, verificación
// por WhatsApp (el código se lee de la cola por SSH), reserva, gestión por enlace y «Cualquier
// barbero». Usa 2 de los 10 códigos de verificación por IP y hora.
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '06 · Sitio público: landing, disponibilidad, verificación y reserva';

const horasDe = r => ['manana', 'tarde', 'noche'].flatMap(f => r[f] ?? []);

export default async function (ctx, t) {
  const a = ctx.anonimo;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;
  const dia = d.fecha(3);

  await t.paso('Landing: caché de 60 s, los servicios y barberos de la prueba y la política de datos', async () => {
    const r = await a.get('/api/v1/publico/landing');
    const l = esperar(r, 200);
    assert.equal(r.cabeceras.get('cache-control'), 'public, max-age=60');
    if (s.corte) assert.ok(l.servicios.some(x => x.id === s.corte), 'el servicio de la prueba no está en la landing');
    if (ctx.creado.barberos.b1) assert.ok(l.barberos.some(x => x.id === ctx.creado.barberos.b1 && x.agendable), 'el barbero de la prueba no es agendable');
    assert.ok(l.politicas.some(p => p.tipo === 'tratamiento_datos'), 'sin política de datos');
    ctx.politica ??= l.politicas.find(p => p.tipo === 'tratamiento_datos').id;
  });

  await t.paso('Servicios agendables y barberos para un servicio', async () => {
    const servicios = esperar(await a.get('/api/v1/publico/servicios'), 200);
    assert.ok(servicios.some(x => x.id === s.corte));
    const barberos = esperar(await a.get(`/api/v1/publico/barberos?servicio_id=${s.corte}`), 200);
    const lista = barberos.barberos ?? barberos;
    for (const id of [b.b1, b.b2, b.b3]) assert.ok(lista.some(x => x.id === id), `falta el barbero ${id}`);
  }, { requiere: [s.corte, b.b3] });

  await t.paso('Calendario del mes y horas del día con un barbero', async () => {
    const cal = esperar(await a.get(`/api/v1/publico/disponibilidad/calendario?servicio_id=${s.corte}&barbero_id=${b.b2}&mes=${dia.slice(0, 7)}`), 200);
    assert.equal(cal.dias.find(x => x.fecha === dia)?.estado, 'disponible');
    const horas = horasDe(esperar(await a.get(`/api/v1/publico/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b2}&fecha=${dia}`), 200));
    assert.equal(horas.length, 47, 'de 00:00 a 23:00 cada 30 minutos');
    ctx.horasPublicas = horas.map(h => h.inicio);
  }, { requiere: [s.corte, b.b2] });

  await t.paso('Horas con «Cualquier barbero»: cada una trae el barbero que se asignaría', async () => {
    const horas = horasDe(esperar(await a.get(`/api/v1/publico/disponibilidad/horas?servicio_id=${s.carrera}&fecha=${dia}`), 200));
    assert.ok(horas.length > 0 && horas.every(h => typeof h.barbero === 'string'));
  }, { requiere: [s.carrera] });

  await t.paso('Parámetros inválidos → 400 o 422', async () => {
    const r = await a.get(`/api/v1/publico/disponibilidad/horas?servicio_id=${s.corte}&fecha=no-es-fecha`);
    assert.ok([400, 422].includes(r.estado), `respondió ${r.estado}`);
  }, { requiere: [s.corte] });

  const verificar = async celular => {
    const pedido = await a.post('/api/v1/publico/verificaciones', { celular }, { reintentar429: false });
    if (pedido.estado === 429) omitir(`límite de 10 códigos de verificación por IP y hora (reintente en ${pedido.cabeceras.get('retry-after')} s)`);
    const r = esperar(pedido, 202, 'pedir el código');
    assert.deepEqual(Object.keys(r).sort(), ['expira_en', 'id', 'reenviar_desde'], 'la respuesta no debe traer datos del cliente');
    const codigo = await ctx.servidor.codigoDeVerificacion(celular);
    return { id: r.id, codigo };
  };

  const celular = d.celular();
  const verificacion = await t.paso('Pedir el código de verificación (se lee de la cola por SSH)', async () => {
    if (!ctx.servidor.disponible) omitir('sin DIAMANTE_SSH no se puede leer el código');
    return verificar(celular);
  });

  await t.paso('Celular inválido → 422', async () => {
    const r = await a.post('/api/v1/publico/verificaciones', { celular: '2001112233' }, { reintentar429: false });
    if (r.estado === 429) omitir('límite de 10 códigos de verificación por IP y hora');
    esperar(r, 422);
  });

  const comprobante = await t.paso('Código equivocado rechazado; el correcto da el comprobante', async () => {
    const malo = String((Number(verificacion.codigo) + 1) % 10000).padStart(4, '0');
    const r1 = await a.post(`/api/v1/publico/verificaciones/${verificacion.id}/confirmar`, { codigo: malo });
    assert.ok([400, 409, 422].includes(r1.estado), `código malo: ${r1.estado}`);
    const r = esperar(await a.post(`/api/v1/publico/verificaciones/${verificacion.id}/confirmar`, { codigo: verificacion.codigo }), 200);
    assert.equal(r.cliente, null, 'cliente nuevo: sin datos');
    return r.comprobante;
  }, { requiere: [verificacion] });

  const reserva = (comp, barbero, inicio, nombreCliente, servicio = s.corte) => ({
    comprobante: comp, servicio_id: servicio, barbero_id: barbero, inicio,
    cliente: { nombre: nombreCliente, correo: null, fecha_nacimiento: null }, autoriza_tratamiento_datos: true, politica_version_id: ctx.politica,
  });

  const sinAutorizar = await t.paso('Sin autorizar el tratamiento de datos → 422', async () => {
    const r = esperar(await a.post('/api/v1/publico/citas', { ...reserva(comprobante, b.b2, ctx.horasPublicas[20], `Andrea ${d.corrida}`), autoriza_tratamiento_datos: false }), 422);
    return r;
  }, { requiere: [comprobante] });

  const cita = await t.paso('Reservar en línea: queda confirmada, con enlace de gestión y Google Calendar', async () => {
    const r = await a.post('/api/v1/publico/citas', reserva(comprobante, b.b2, ctx.horasPublicas[20], `Andrea ${d.corrida}`));
    const c = esperar(r, 201);
    assert.deepEqual([c.cita.estado, c.cita.origen], ['confirmada', 'en_linea']);
    assert.match(c.calendario_google, /ctz=America%2FBogota/);
    assert.match(r.cabeceras.get('location') ?? '', /\/publico\/citas\/gestion\//);
    ctx.creado.citas.enLinea = c.cita.id;
    return { id: c.cita.id, token: c.enlace_gestion.split('/').pop() };
  }, { requiere: [comprobante, sinAutorizar] });

  await t.paso('El comprobante ya usado no sirve para otra reserva', async () => {
    const r = await a.post('/api/v1/publico/citas', reserva(comprobante, b.b2, ctx.horasPublicas[22], `Andrea ${d.corrida}`));
    assert.ok([409, 410, 422].includes(r.estado), `comprobante reutilizado: ${r.estado}`);
  }, { requiere: [cita] });

  await t.paso('Gestión por enlace: ver (privado), recordatorios, reprogramar y cancelar', async () => {
    const ruta = `/api/v1/publico/citas/gestion/${cita.token}`;
    const ver = await a.get(ruta);
    const g = esperar(ver, 200);
    assert.equal(ver.cabeceras.get('cache-control'), 'private, no-store');
    assert.equal(g.puede_cancelar, true);
    assert.ok(!('cliente' in g.cita), 'el enlace no debe mostrar datos de contacto');
    esperar(await a.put(`${ruta}/recordatorios`, { activos: false }), 200);
    const otraHora = ctx.horasPublicas[26];
    const reprogramada = esperar(await a.post(`${ruta}/reprogramar`, { inicio: otraHora, barbero_id: b.b2 }), 200);
    const nuevoToken = (reprogramada.enlace_gestion ?? `${ruta}`).split('/').pop();
    const rutaNueva = `/api/v1/publico/citas/gestion/${nuevoToken}`;
    if (nuevoToken !== cita.token) esperarError(await a.get(ruta), 410, 'enlace_vencido');
    const cancelada = esperar(await a.post(`${rutaNueva}/cancelar`), 200);
    assert.equal(cancelada.cita?.estado ?? cancelada.estado, 'cancelada');
  }, { requiere: [cita] });

  await t.paso('Enlace inexistente → 410 enlace_vencido', async () => {
    esperarError(await a.get('/api/v1/publico/citas/gestion/no-existe-este-enlace'), 410, 'enlace_vencido');
  });

  await t.paso('Reserva con «Cualquier barbero»: asignación automática entre los que ofrecen el servicio', async () => {
    if (!ctx.servidor.disponible) omitir('sin DIAMANTE_SSH');
    const v = await verificar(d.celular());
    const comp = esperar(await a.post(`/api/v1/publico/verificaciones/${v.id}/confirmar`, { codigo: v.codigo }), 200).comprobante;
    const c = esperar(await a.post('/api/v1/publico/citas', reserva(comp, null, ctx.horasPublicas[30], `Bruno ${d.corrida}`, s.carrera)), 201);
    assert.equal(c.cita.barbero.asignacion_automatica, true);
    assert.ok([b.b2, b.b3].includes(c.cita.barbero.id), 'asignó un barbero que no ofrece el servicio');
  }, { requiere: [s.carrera, ctx.horasPublicas] });
}
