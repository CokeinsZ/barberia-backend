// Equipo (RF-BAR): registro, edición, visibilidad, orden, horario, servicios, datos privados
// cifrados, foto, expediente con descargas firmadas, ausencias con soporte y bloqueos.
// Tres barberos con horario de 00:00 a 23:30 todos los días (para que siempre haya turnos de hoy).
import { assert, esperar, esperarError } from '../lib/registro.mjs';

export const nombre = '03 · Barberos: equipo, horario, foto, expediente, ausencias y bloqueos';

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;
  const s = ctx.creado.servicios;
  const b = ctx.creado.barberos;

  const datosBarbero = (nombres, correo, servicios, acceso) => ({
    tipo_documento: 'CC', numero_documento: d.documento(), nombres, apellidos: `Prueba ${d.corrida}`,
    fecha_nacimiento: '1995-03-02', celular: '310 555 9917', correo, especialidad: 'Fade y diseños',
    anios_experiencia: 6, instagram: '@prueba.e2e', biografia: 'Barbero de las pruebas E2E.', fecha_ingreso: '2025-01-15',
    tipo_vinculacion: 'prestacion_servicios', periodo_liquidacion: 'quincenal', porcentaje_comision: 40,
    servicios, horario: d.horarioCompleto,
    visibilidad: { mostrar_en_landing: true, recibe_citas_en_linea: true, acceso_panel: acceso },
  });
  ctx.datosBarbero = {};

  for (const [clave, nombres, servicios, acceso] of [
    ['b1', 'Santiago', [s.corte, s.barba, s.cejas, s.combo], true],
    ['b2', 'Mateo', [s.corte, s.barba, s.carrera], false],
    ['b3', 'Daniel', [s.corte, s.carrera], false],
  ]) {
    await t.paso(`Registrar a ${nombres}${acceso ? ' (con acceso al panel)' : ''}`, async () => {
      const cuerpo = datosBarbero(nombres, d.correo(`barbero-${clave}`), servicios.filter(Boolean), acceso);
      const r = esperar(await admin.post('/api/v1/barberos', cuerpo), 201);
      b[clave] = r.id;
      ctx.datosBarbero[clave] = cuerpo;
      assert.equal(r.estado?.codigo, 'activo');
      assert.equal(r.horario?.total_horas_semana, 164.5);
    }, { requiere: [s.corte] });
  }

  await t.paso('Documento repetido → 409 y datos inválidos → 422', async () => {
    const repetido = { ...ctx.datosBarbero.b2, correo: d.correo('repetido') };
    esperar(await admin.post('/api/v1/barberos', repetido), 409);
    const r = esperar(await admin.post('/api/v1/barberos', { ...ctx.datosBarbero.b2, numero_documento: '1', correo: 'no', celular: '123', horario: [{ dia_semana: 1, inicio: '13:00', fin: '09:00' }] }), 422);
    assert.ok(r.errores.length >= 3);
  }, { requiere: [b.b2] });

  await t.paso('Listar el equipo y filtrar', async () => {
    const r = esperar(await admin.get('/api/v1/barberos'), 200);
    for (const id of [b.b1, b.b2, b.b3]) assert.ok(r.datos.some(x => x.id === id), `falta el barbero ${id}`);
    const q = esperar(await admin.get(`/api/v1/barberos?q=${encodeURIComponent(`Prueba ${d.corrida}`)}`), 200);
    assert.equal(q.datos.length, 3);
    // El orden se guarda con todos los barberos vigentes: los que no tienen fecha de retiro (uno
    // retirado hoy aún figura «activo» hoy, pero ya no entra en el orden).
    ctx.ordenEquipo = r.datos.filter(x => !x.fecha_retiro).map(x => x.id);
  }, { requiere: [b.b1, b.b2, b.b3] });

  await t.paso('Ficha: invitación pendiente para quien tiene acceso al panel', async () => {
    const f = esperar(await admin.get(`/api/v1/barberos/${b.b1}`), 200);
    assert.equal(f.cuenta?.pendiente_activacion, true);
  }, { requiere: [b.b1] });

  await t.paso('Editar los datos esenciales (comisión al 45 %)', async () => {
    const { servicios, horario, visibilidad, ...esenciales } = ctx.datosBarbero.b1;
    const r = esperar(await admin.put(`/api/v1/barberos/${b.b1}`, { ...esenciales, porcentaje_comision: 45 }), 200);
    assert.equal(Number(r.porcentaje_comision ?? r.comision?.porcentaje ?? 45), 45);
  }, { requiere: [b.b1] });

  await t.paso('Visibilidad y orden del equipo (sin cambiar el orden actual)', async () => {
    esperar(await admin.put(`/api/v1/barberos/${b.b2}/visibilidad`, { mostrar_en_landing: true, recibe_citas_en_linea: true, acceso_panel: false }), 200);
    esperar(await admin.put('/api/v1/barberos/orden', { barbero_ids: ctx.ordenEquipo }), [200, 204]);
  }, { requiere: [ctx.ordenEquipo] });

  await t.paso('Horario y servicios: leer y guardar', async () => {
    const h = esperar(await admin.get(`/api/v1/barberos/${b.b3}/horario`), 200);
    assert.ok(JSON.stringify(h).includes('23:30'));
    esperar(await admin.put(`/api/v1/barberos/${b.b3}/horario`, { franjas: d.horarioCompleto }), 200);
    esperar(await admin.put(`/api/v1/barberos/${b.b3}/horario`, { franjas: [{ dia_semana: 1, inicio: '09:00', fin: '13:00' }, { dia_semana: 1, inicio: '12:00', fin: '18:00' }] }), 422);
    esperar(await admin.get(`/api/v1/barberos/${b.b3}/servicios`), 200);
    esperar(await admin.put(`/api/v1/barberos/${b.b3}/servicios`, { servicio_ids: [s.corte, s.carrera] }), 200);
  }, { requiere: [b.b3] });

  await t.paso('Datos privados cifrados: guardar y leer igual', async () => {
    const privados = { direccion: 'Cra. 15 # 93-47', ciudad: 'Bogotá', grupo_sanguineo: 'O+', emergencia: { nombre: 'Marta Prueba', parentesco: 'madre_padre', celular: '3001112233' }, eps: 'Sanitas', medio_pago: 'nequi', numero_cuenta: '3001112233' };
    esperar(await admin.put(`/api/v1/barberos/${b.b1}/datos-privados`, privados), 200);
    const r = esperar(await admin.get(`/api/v1/barberos/${b.b1}/datos-privados`), 200);
    assert.equal(r.grupo_sanguineo, 'O+');
    assert.equal(r.numero_cuenta, '3001112233');
    esperar(await admin.put(`/api/v1/barberos/${b.b1}/datos-privados`, { grupo_sanguineo: 'Z+', medio_pago: 'bitcoin' }), 422);
  }, { requiere: [b.b1] });

  await t.paso('Foto: se sube y se ve en /media/publico', async () => {
    const r = esperar(await admin.subir(`/api/v1/barberos/${b.b1}/foto`, { foto: { nombre: 'santiago.png', tipo: 'image/png', contenido: d.fotoPng() } }), 200);
    assert.match(r.foto_url, /\/media\/publico\//);
    const foto = await ctx.anonimo.get(r.foto_url);
    esperar(foto, 200, 'foto pública');
    assert.equal(foto.cabeceras.get('content-type'), 'image/png');
    assert.match(foto.cabeceras.get('cache-control') ?? '', /max-age/);
  }, { requiere: [b.b1] });

  await t.paso('Una imagen horizontal o un archivo que no es imagen se rechazan', async () => {
    const horizontal = await admin.subir(`/api/v1/barberos/${b.b1}/foto`, { foto: { nombre: 'h.png', tipo: 'image/png', contenido: d.fotoPng(400, 300) } });
    assert.ok([415, 422].includes(horizontal.estado), `horizontal: ${horizontal.estado}`);
    const falso = await admin.subir(`/api/v1/barberos/${b.b1}/foto`, { foto: { nombre: 'x.png', tipo: 'image/png', contenido: Buffer.from('no soy una imagen') } });
    assert.ok([415, 422].includes(falso.estado), `no imagen: ${falso.estado}`);
  }, { requiere: [b.b1] });

  await t.paso('Expediente: cargar un contrato, descargarlo con URL firmada y quitarlo', async () => {
    const r = esperar(await admin.subir(`/api/v1/barberos/${b.b1}/documentos`, { tipo: 'contrato', archivo: { nombre: 'contrato.pdf', tipo: 'application/pdf', contenido: d.pdf('Contrato E2E') } }), 201);
    const exp = esperar(await admin.get(`/api/v1/barberos/${b.b1}/documentos`), 200);
    const doc = exp.documentos.find(x => x.id === r.id);
    assert.ok(doc?.url_descarga.includes('firma='), 'sin URL firmada');
    const descarga = await ctx.anonimo.get(doc.url_descarga);
    esperar(descarga, 200, 'descarga firmada');
    assert.ok(descarga.bytes.subarray(0, 4).toString() === '%PDF');
    assert.match(descarga.cabeceras.get('content-disposition') ?? '', /attachment/);
    const alterada = await ctx.anonimo.get(doc.url_descarga.replace(/firma=[^&]+/, 'firma=alterada'));
    assert.ok([403, 404, 410].includes(alterada.estado), `firma alterada: ${alterada.estado}`);
    esperar(await admin.get(`/api/v1/barberos/${b.b1}/documentos/${r.id}/descarga`), [200, 303]);
    esperar(await admin.delete(`/api/v1/barberos/${b.b1}/documentos/${r.id}`), 204);
  }, { requiere: [b.b1] });

  await t.paso('Ausencia con soporte, y anularla', async () => {
    const a = esperar(await admin.post(`/api/v1/barberos/${b.b2}/ausencias`, { tipo: 'vacaciones', desde: d.fecha(45), hasta: d.fecha(46), observacion: `Vacaciones ${d.sufijo}` }), 201);
    esperarError(await admin.post(`/api/v1/barberos/${b.b2}/ausencias`, { tipo: 'licencia', desde: d.fecha(46), hasta: d.fecha(47), observacion: 'Se cruza' }), 409, 'ausencia_solapada');
    esperar(await admin.subir(`/api/v1/barberos/${b.b2}/ausencias/${a.id}/soporte`, { soporte: { nombre: 'soporte.pdf', tipo: 'application/pdf', contenido: d.pdf('Soporte') } }), 200);
    const lista = esperar(await admin.get(`/api/v1/barberos/${b.b2}/ausencias`), 200);
    assert.ok(JSON.stringify(lista).includes(String(a.id)));
    esperar(await admin.post(`/api/v1/barberos/${b.b2}/ausencias/${a.id}/anular`), 200);
  }, { requiere: [b.b2] });

  await t.paso('Bloqueo: crear, cruce rechazado, listar y quitar', async () => {
    const dia = d.fecha(13);
    const r = esperar(await admin.post(`/api/v1/barberos/${b.b3}/bloqueos`, { fecha: dia, desde: '14:00', hasta: '16:00', motivo: 'cita_medica', nota: 'Odontólogo' }), 201);
    esperar(await admin.post(`/api/v1/barberos/${b.b3}/bloqueos`, { fecha: dia, desde: '15:00', hasta: '17:00', motivo: 'otro' }), 409);
    esperar(await admin.post(`/api/v1/barberos/${b.b3}/bloqueos`, { fecha: '2020-01-01', desde: '10:00', hasta: '11:00', motivo: 'otro' }), 422);
    esperar(await admin.get(`/api/v1/barberos/${b.b3}/bloqueos`), 200);
    esperar(await admin.delete(`/api/v1/barberos/${b.b3}/bloqueos/${r.id}`), 204);
  }, { requiere: [b.b3] });

  await t.paso('Citas afectadas por un cambio propuesto (conflictos)', async () => {
    const r = esperar(await admin.post('/api/v1/agenda/conflictos', { barbero_id: b.b3, ausencia: { desde: d.fecha(13), hasta: d.fecha(13) } }), 200);
    assert.ok(Array.isArray(r.citas ?? r));
  }, { requiere: [b.b3] });

  await t.paso('Barbero inexistente → 404', async () => {
    esperar(await admin.get('/api/v1/barberos/999999999'), 404);
  });
}
