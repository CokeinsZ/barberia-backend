// Negocio (RF-CFG-01 a 05) y catálogo de servicios (RF-SER). Los datos del negocio se guardan
// sin cambios (no se altera la preproducción); los días especiales de prueba se borran.
import { assert, esperar, esperarError } from '../lib/registro.mjs';

export const nombre = '02 · Negocio y catálogo de servicios';

export default async function (ctx, t) {
  const admin = ctx.admin;
  const { d } = ctx;

  const negocio = await t.paso('Leer los datos del negocio', async () => esperar(await admin.get('/api/v1/negocio'), 200));

  await t.paso('Guardar los mismos datos (idempotente y auditado)', async () => {
    const antes = esperar(await admin.get('/api/v1/auditoria?entidad=negocio'), 200).total;
    esperar(await admin.put('/api/v1/negocio', negocio), 200);
    const despues = esperar(await admin.get('/api/v1/auditoria?entidad=negocio'), 200).total;
    assert.equal(despues, antes + 1, 'el cambio no quedó en la auditoría');
  }, { requiere: [negocio] });

  await t.paso('Datos del negocio inválidos → 422; campos desconocidos → rechazados', async () => {
    const r = esperar(await admin.put('/api/v1/negocio', { nombre_comercial: 'D', nit: '123', porcentaje_comision_defecto: 120 }), 422);
    assert.ok(r.errores.length >= 2);
    const r2 = await admin.put('/api/v1/negocio', { ...negocio, campo_raro: true });
    assert.ok([400, 422].includes(r2.estado), `campo desconocido: ${r2.estado}`);
  }, { requiere: [negocio] });

  await t.paso('Horario de atención', async () => {
    esperar(await admin.get('/api/v1/negocio/horario-atencion'), 200);
  });

  const lejano = d.fecha(95);
  const dia = await t.paso('Crear un día especial (cierre temprano)', async () => {
    const r = await admin.post('/api/v1/negocio/dias-especiales', { fecha: lejano, nombre: `Cierre ${d.sufijo}`, tipo: 'cierre_temprano', hora_cierre: '14:00:00' });
    return esperar(r, 201).id;
  });

  await t.paso('Cierre temprano sin hora → 422', async () => {
    esperar(await admin.post('/api/v1/negocio/dias-especiales', { fecha: d.fecha(96), nombre: 'Sin hora', tipo: 'cierre_temprano' }), 422);
  });

  await t.paso('Editar, listar y borrar el día especial', async () => {
    esperar(await admin.put(`/api/v1/negocio/dias-especiales/${dia}`, { fecha: lejano, nombre: `Cerrado ${d.sufijo}`, tipo: 'cerrado' }), 200);
    const lista = esperar(await admin.get(`/api/v1/negocio/dias-especiales?desde=${lejano}&hasta=${lejano}`), 200);
    assert.ok((Array.isArray(lista) ? lista : lista.datos).some(x => x.id === dia));
    esperar(await admin.delete(`/api/v1/negocio/dias-especiales/${dia}`), 204);
    esperar(await admin.delete(`/api/v1/negocio/dias-especiales/${dia}`), 404);
  }, { requiere: [dia] });

  await t.paso('Precargar los festivos del año siguiente (idempotente) y rechazar un año absurdo', async () => {
    const anio = Number(d.hoy().slice(0, 4)) + 1;
    esperar(await admin.post(`/api/v1/negocio/dias-especiales/festivos/${anio}`), [200, 201]);
    esperar(await admin.post(`/api/v1/negocio/dias-especiales/festivos/${anio}`), [200, 201]);
    esperar(await admin.post('/api/v1/negocio/dias-especiales/festivos/2100'), 422);
  });

  await t.paso('Política de tratamiento de datos vigente (se crea si falta)', async () => {
    const politicas = esperar(await admin.get('/api/v1/negocio/politicas'), 200);
    let vigente = politicas.find(p => p.tipo === 'tratamiento_datos' && p.vigente);
    if (!vigente) {
      vigente = esperar(await admin.post('/api/v1/negocio/politicas', { tipo: 'tratamiento_datos', version: '1.0', url: '/legal/tratamiento-de-datos' }), 201);
    }
    ctx.politica = vigente.id;
  });

  const categorias = await t.paso('Categorías', async () => {
    const c = esperar(await admin.get('/api/v1/categorias'), 200);
    assert.ok(c.length > 0, 'no hay categorías');
    return c;
  });
  const categoria = categorias?.[0]?.id;

  const servicio = (nombreServicio, precio) => ({ categoria_id: categoria, nombre: `${nombreServicio} ${d.sufijo}`, descripcion: `${nombreServicio} de prueba.`, precio, duracion_min: 30 });
  for (const [clave, nombreServicio, precio] of [['corte', 'Corte', 35000], ['barba', 'Barba', 25000], ['cejas', 'Cejas', 10000], ['carrera', 'Corte carrera', 30000]]) {
    await t.paso(`Crear el servicio «${nombreServicio}»`, async () => {
      const r = esperar(await admin.post('/api/v1/servicios', servicio(nombreServicio, precio)), 201);
      ctx.creado.servicios[clave] = r.id;
    }, { requiere: [categoria] });
  }
  const s = ctx.creado.servicios;

  await t.paso('Nombre repetido (sin tildes ni mayúsculas) → 409', async () => {
    const r = await admin.post('/api/v1/servicios', { ...servicio('CORTE', 1000) });
    esperar(r, 409);
  }, { requiere: [s.corte] });

  await t.paso('Servicio inválido → 422', async () => {
    esperar(await admin.post('/api/v1/servicios', { categoria_id: categoria, nombre: 'X', descripcion: '', precio: 0, duracion_min: 47 }), 422);
  }, { requiere: [categoria] });

  await t.paso('Combo con componentes; cambiarlos (con uno solo no es combo → 422)', async () => {
    const r = esperar(await admin.post('/api/v1/servicios', {
      tipo: 'combo', categoria_id: categoria, nombre: `Corte + cejas ${d.sufijo}`, descripcion: 'Combo de prueba.',
      precio: 40000, duracion_min: 60, destacado: true, etiqueta: 'Ahorras $5.000', componentes: [s.corte, s.cejas],
    }), 201);
    s.combo = r.id;
    esperar(await admin.put(`/api/v1/servicios/${s.combo}/componentes`, { servicio_ids: [s.corte, s.barba] }), 200);
    esperar(await admin.put(`/api/v1/servicios/${s.combo}/componentes`, { servicio_ids: [s.corte] }), 422);
  }, { requiere: [s.corte, s.cejas, s.barba] });

  await t.paso('Listar, ver y editar un servicio (precio nuevo)', async () => {
    const lista = esperar(await admin.get('/api/v1/servicios'), 200);
    assert.ok(lista.some(x => x.id === s.corte));
    esperar(await admin.get(`/api/v1/servicios/${s.corte}`), 200);
    const r = esperar(await admin.put(`/api/v1/servicios/${s.corte}`, { ...servicio('Corte', 38000) }), 200);
    assert.equal(r.precio, 38000);
  }, { requiere: [s.corte] });

  await t.paso('Desactivar y reactivar un servicio', async () => {
    esperar(await admin.patch(`/api/v1/servicios/${s.barba}/estado`, { activo: false }), 200);
    const inactivos = esperar(await admin.get('/api/v1/servicios?incluir_inactivos=true'), 200);
    assert.equal(inactivos.find(x => x.id === s.barba)?.activo, false);
    esperar(await admin.patch(`/api/v1/servicios/${s.barba}/estado`, { activo: true }), 200);
  }, { requiere: [s.barba] });

  await t.paso('Servicio inexistente → 404', async () => {
    esperarError(await admin.get('/api/v1/servicios/999999999'), 404);
  });
}
