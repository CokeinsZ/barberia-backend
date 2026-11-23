// Entrega 2 (fase 8): umbrales de los segmentos (se dejan como estaban), indicadores de la base,
// «Mis citas» del barbero, anular y volver a cobrar una cita, aviso al retirar a un barbero con
// citas, vigilancia de la operación y entradas hostiles por la red (detrás de Traefik).
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '12b · Entrega 2: segmentos, «Mis citas», ventas y vigilancia';

const horasDe = r => ['manana', 'tarde', 'noche'].flatMap(f => r[f] ?? []).map(h => h.inicio ?? h);

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;
  const ruta = '/api/v1/clientes/configuracion-segmentos';

  const originales = await t.paso('Umbrales de los segmentos: leer y rechazar valores inválidos', async () => {
    const u = esperar(await admin.get(ruta), 200);
    assert.deepEqual(Object.keys(u).sort(), ['dias_inactividad', 'max_visitas_nuevo', 'max_visitas_recurrente']);
    const r = esperar(await admin.put(ruta, { max_visitas_nuevo: 3, max_visitas_recurrente: 3, dias_inactividad: 10 }), 422);
    assert.deepEqual(r.errores.map(e => e.campo), ['max_visitas_recurrente', 'dias_inactividad']);
    return u;
  });

  await t.paso('Cambiar los umbrales cambia los segmentos y los reportes al instante (y se restauran)', async () => {
    const otros = {
      max_visitas_nuevo: originales.max_visitas_nuevo === 1 ? 2 : 1,
      max_visitas_recurrente: originales.max_visitas_recurrente === 50 ? 40 : 50,
      dias_inactividad: originales.dias_inactividad === 30 ? 31 : 30,
    };
    try {
      assert.deepEqual(esperar(await admin.put(ruta, otros), 200), otros);
      const listado = esperar(await admin.get('/api/v1/clientes?por_pagina=1'), 200);
      assert.equal(listado.meta.dias_inactividad, otros.dias_inactividad);
      const reporte = esperar(await admin.get('/api/v1/reportes/clientes'), 200);
      assert.equal(reporte.por_reactivar.dias_inactividad, otros.dias_inactividad);
      assert.equal(reporte.por_reactivar.total, listado.meta.por_segmento.inactivo, 'reportes y clientes cuentan igual');
    } finally {
      esperar(await admin.put(ruta, originales), 200);
    }
    const auditoria = esperar(await admin.get('/api/v1/auditoria?entidad=configuracion_segmentos&por_pagina=5'), 200);
    assert.ok(auditoria.datos.length >= 2 && auditoria.datos.every(x => x.accion === 'cambiar_segmentos'));
  }, { requiere: [originales] });

  await t.paso('Indicadores de la base: los conteos (SQL) cuadran con el filtro por segmento (dominio)', async () => {
    const listado = esperar(await admin.get('/api/v1/clientes?por_pagina=1'), 200);
    const m = listado.meta;
    assert.equal(m.registrados, listado.total);
    assert.equal(Object.values(m.por_segmento).reduce((a, n) => a + n, 0), m.registrados);
    for (const segmento of Object.keys(m.por_segmento)) {
      const filtrado = esperar(await admin.get(`/api/v1/clientes?segmento=${segmento}&por_pagina=1`), 200);
      assert.equal(filtrado.total, m.por_segmento[segmento], segmento);
      assert.deepEqual(filtrado.meta, m, 'los indicadores no cambian con los filtros');
    }
  });

  await t.paso('«Mis citas» del barbero: hoy, mañana y 7 días; el tiempo libre cuadra', async () => {
    if (!ctx.barbero) omitir('sin sesión de barbero (escenario 04)');
    const r = esperar(await ctx.barbero.get('/api/v1/agenda/resumen-barbero'), 200);
    assert.equal(r.barbero_id, b.b1);
    assert.equal(r.hoy.fecha, d.hoy());
    assert.equal(r.proximos_7_dias.dias.length, 7);
    assert.deepEqual(r.proximos_7_dias.dias[0], r.manana);
    for (const dia of [r.hoy, ...r.proximos_7_dias.dias]) {
      // Sus franjas: de 00:00 a 23:30 todos los días (1.410 minutos), menos lo bloqueado.
      assert.ok(dia.minutos_disponibles <= 1410, JSON.stringify(dia));
      if (dia.minutos_libres !== null) assert.equal(dia.minutos_libres, dia.minutos_disponibles - dia.minutos_ocupados);
    }
    assert.equal(r.proximos_7_dias.dias_con_agenda_llena, r.proximos_7_dias.dias.filter(x => x.agenda_llena).length);
    const delAdmin = esperar(await admin.get(`/api/v1/agenda/resumen-barbero?barbero_id=${b.b1}`), 200);
    assert.deepEqual(delAdmin.proximos_7_dias, r.proximos_7_dias, 'la administradora ve lo mismo');
    esperarError(await admin.get('/api/v1/agenda/resumen-barbero'), 422, 'validacion');
  }, { requiere: [b.b1] });

  await t.paso('Anular la venta de una cita la deja por cobrar; se vuelve a cobrar una sola vez', async () => {
    const cat = ctx.metodos;
    if (!cat) omitir('sin los catálogos de ventas (escenario 08)');
    const horas = esperar(await admin.get(`/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte}&barbero_id=${b.b2}&fecha=${d.hoy()}`), 200);
    const inicio = horasDe(horas)[0];
    if (!inicio) omitir('no quedan turnos hoy');
    const precio = esperar(await admin.get(`/api/v1/servicios/${s.corte}`), 200).precio;
    const c = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Rocío Peña ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: b.b2, inicio }), 201);
    const cobro = { cita_id: c.id, servicio_ids: [s.corte], pagos: [{ metodo_pago_id: cat.efectivo, monto: precio }] };
    const venta = esperar(await admin.post('/api/v1/ventas', cobro), 201);
    assert.equal(esperar(await admin.get(`/api/v1/agenda/citas/${c.id}`), 200).estado, 'atendida');
    esperar(await admin.post(`/api/v1/ventas/${venta.id}/anulacion`, { motivo: 'Se cobró el servicio equivocado' }), 200);
    const detalle = esperar(await admin.get(`/api/v1/agenda/citas/${c.id}`), 200);
    assert.equal(detalle.estado, 'confirmada');
    assert.equal(detalle.historial.at(-1).evento, 'venta_anulada');
    esperar(await admin.post('/api/v1/ventas', cobro), 201);
    esperarError(await admin.post('/api/v1/ventas', cobro), 409, 'cita_ya_cobrada');
  }, { requiere: [s.corte, b.b2] });

  await t.paso('Retirar a un barbero con citas pendientes avisa a la administradora', async () => {
    // Un barbero propio de este paso: los demás escenarios siguen con el equipo de la corrida.
    const nuevo = esperar(await admin.post('/api/v1/barberos', {
      tipo_documento: 'CC', numero_documento: d.documento(), nombres: 'Retiro', apellidos: `Prueba ${d.corrida}`,
      fecha_nacimiento: '1995-03-02', celular: '310 555 9917', correo: d.correo('barbero-retiro'), especialidad: 'Fade',
      anios_experiencia: 3, fecha_ingreso: '2025-01-15', tipo_vinculacion: 'prestacion_servicios',
      servicios: [s.corte], horario: d.horarioCompleto,
      visibilidad: { mostrar_en_landing: false, recibe_citas_en_linea: false, acceso_panel: false },
    }), 201);
    const cita = esperar(await admin.post('/api/v1/agenda/citas', { cliente_nuevo: { nombre: `Simón Gil ${d.corrida}`, celular: d.celular() }, servicio_id: s.corte, barbero_id: nuevo.id, inicio: d.instante(d.fecha(2), '10:00') }), 201);
    esperar(await admin.post(`/api/v1/barberos/${nuevo.id}/retiro`, { fecha: d.fecha(1), motivo: 'renuncia_voluntaria' }), 200);
    const avisos = esperar(await admin.get('/api/v1/avisos?no_leidos=true&por_pagina=100'), 200);
    const aviso = avisos.datos.find(a => (a.datos?.cita_ids ?? []).includes(cita.id));
    assert.ok(aviso, 'no apareció el aviso de la cita del barbero retirado');
    assert.match(aviso.titulo, /retiro/);
    // Se deja limpio: la cita cancelada y el aviso leído.
    esperar(await admin.post(`/api/v1/agenda/citas/${cita.id}/cancelar`, { motivo: 'barbero_no_disponible', avisar_cliente: false }), 200);
    esperar(await admin.post(`/api/v1/avisos/${aviso.id}/leido`), [200, 204]);
  }, { requiere: [s.corte] });

  await t.paso('Vigilancia: el monitor externo ve «ok» y la administradora, el detalle sin alertas', async () => {
    const estado = esperar(await admin.get('/api/v1/sistema/estado'), 200);
    const alertas = estado.alertas.map(a => `${a.codigo}: ${a.mensaje}`).join(' | ');
    const monitor = await ctx.anonimo.get('/api/salud/operacion');
    assert.equal(monitor.estado, 200, `alertas en preproducción: ${alertas || JSON.stringify(monitor.json)}`);
    assert.deepEqual(monitor.json, { estado: 'ok' });
    assert.equal(estado.estado, 'ok');
    assert.ok(estado.cola.correo.configurado, 'sin SMTP configurado');
    assert.ok(estado.api.desde_el_arranque.peticiones > 0);
    assert.ok(estado.api.rutas_destacadas.every(r => !/\d{3,}/.test(r.ruta)), 'las rutas deben ser plantillas, sin identificadores');
    if (ctx.barbero) esperar(await ctx.barbero.get('/api/v1/sistema/estado'), 403);
  });

  await t.paso('Entradas hostiles por la red (detrás de Traefik): 4xx, nunca 5xx', async () => {
    const casos = [
      [admin, 'POST', '/api/v1/clientes', { nombre: 'Ana\u0000 Ruiz', celular: d.celular() }, 422],
      [admin, 'GET', '/api/v1/agenda/tablero?fecha=%2B262142-12-31', undefined, 422],
      [ctx.anonimo, 'GET', `/api/v1/publico/disponibilidad/horas?servicio_id=${s.corte}&fecha=%2B262142-12-31`, undefined, 422],
      [ctx.anonimo, 'POST', '/api/v1/auth/sesion', { correo: 'a\u0000@example.com', contrasena: 'x' }, 422],
      [admin, 'POST', '/api/v1/ventas', `{"cliente":{"tipo":"ocasional"},"barbero_id":${b.b2},"servicio_ids":[${s.corte}],"pagos":[{"metodo_pago_id":1,"monto":9223372036854775807}]}`, 422],
      [admin, 'GET', '/api/v1/clientes/%00', undefined, null],
      [admin, 'GET', `/api/v1/clientes?q=${'x'.repeat(4000)}`, undefined, null],
    ];
    for (const [cliente, metodo, ruta, cuerpo, esperado] of casos) {
      const r = await cliente.pedir(metodo, ruta, { cuerpo, reintentar429: false });
      const que = `${metodo} ${ruta.slice(0, 80)} → ${r.estado}`;
      assert.ok(r.estado < 500, que);
      if (esperado) assert.equal(r.estado, esperado, que);
    }
  }, { requiere: [s.corte, b.b2] });
}
