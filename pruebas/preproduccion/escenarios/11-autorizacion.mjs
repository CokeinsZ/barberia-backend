// Autorización (ASVS V4): sin sesión, toda ruta del panel responde 401; con sesión de barbero,
// las de la administradora 403 o 404; la administradora nunca recibe 401 ni 403.
import { assert, esperar } from '../lib/registro.mjs';

export const nombre = '11 · Matriz de autorización';

export default async function (ctx, t) {
  const b = ctx.creado.barberos;
  const s = ctx.creado.servicios;
  const hoy = ctx.d.hoy();
  const otro = b.b2 ?? 1;
  const deAdministradora = [
    '/api/v1/administradores', '/api/v1/negocio', '/api/v1/negocio/politicas', '/api/v1/auditoria', '/api/v1/barberos',
    `/api/v1/barberos/${otro}`, `/api/v1/barberos/${otro}/datos-privados`, `/api/v1/barberos/${otro}/documentos`,
    `/api/v1/barberos/${otro}/ausencias`, '/api/v1/clientes', `/api/v1/clientes/${ctx.creado.clientes.diana?.id ?? 1}`,
    '/api/v1/clientes/solicitudes-titular', `/api/v1/agenda/tablero?fecha=${hoy}`, '/api/v1/agenda/pendientes-de-cierre',
    '/api/v1/agenda/configuracion', `/api/v1/agenda/disponibilidad/horas?servicio_id=${s.corte ?? 1}&fecha=${hoy}`,
    '/api/v1/avisos', '/api/v1/mensajes', '/api/v1/ventas/caja', `/api/v1/ventas/cierres?fecha=${hoy}`, '/api/v1/ventas/metodos-pago',
    '/api/v1/reportes/dashboard', '/api/v1/reportes/tendencias', '/api/v1/reportes/alertas', '/api/v1/reportes/resumen',
    '/api/v1/reportes/clientes', `/api/v1/reportes/barberos/${otro}`,
  ];
  const delPersonal = ['/api/v1/auth/sesion', '/api/v1/categorias', '/api/v1/servicios', '/api/v1/negocio/horario-atencion',
    '/api/v1/negocio/dias-especiales', '/api/v1/clientes/buscar?q=ana', '/api/v1/agenda/citas', '/api/v1/ventas', '/api/v1/ventas/catalogos',
    '/api/v1/ventas/resumen-barbero'];
  const escrituras = [
    ['POST', '/api/v1/servicios'], ['POST', '/api/v1/barberos'], ['POST', '/api/v1/clientes'], ['POST', '/api/v1/agenda/citas'],
    ['POST', '/api/v1/ventas'], ['PUT', '/api/v1/negocio'], ['POST', '/api/v1/administradores'], ['PUT', '/api/v1/agenda/configuracion'],
    ['POST', '/api/v1/ventas/cierres'], ['POST', `/api/v1/barberos/${otro}/retiro`], ['POST', '/api/v1/agenda/citas/reasignacion'],
  ];

  await t.paso(`Sin sesión: ${deAdministradora.length + delPersonal.length + escrituras.length} rutas del panel responden 401`, async () => {
    const fallas = [];
    for (const ruta of [...deAdministradora, ...delPersonal]) {
      const r = await ctx.anonimo.get(ruta);
      if (r.estado !== 401 || r.json?.codigo !== 'no_autenticado') fallas.push(`GET ${ruta} → ${r.estado}`);
    }
    for (const [metodo, ruta] of escrituras) {
      const r = await ctx.anonimo.pedir(metodo, ruta, { cuerpo: {} });
      if (r.estado !== 401) fallas.push(`${metodo} ${ruta} → ${r.estado}`);
    }
    assert.deepEqual(fallas, []);
  });

  await t.paso(`Barbero: ${deAdministradora.length} rutas de la administradora responden 403 o 404`, async () => {
    if (!ctx.barbero) return;
    const fallas = [];
    for (const ruta of deAdministradora) {
      const r = await ctx.barbero.get(ruta);
      if (![403, 404].includes(r.estado)) fallas.push(`GET ${ruta} → ${r.estado}`);
    }
    for (const ruta of delPersonal) {
      const r = await ctx.barbero.get(ruta);
      if ([401, 403].includes(r.estado) || r.estado >= 500) fallas.push(`GET ${ruta} (del personal) → ${r.estado}`);
    }
    assert.deepEqual(fallas, []);
  });

  await t.paso('La administradora no recibe 401, 403 ni 5xx en ninguna lectura', async () => {
    const fallas = [];
    for (const ruta of [...deAdministradora, ...delPersonal]) {
      const r = await ctx.admin.get(ruta);
      if ([401, 403].includes(r.estado) || r.estado >= 500) fallas.push(`GET ${ruta} → ${r.estado}`);
    }
    assert.deepEqual(fallas, []);
  });

  await t.paso('Una cookie de sesión inventada no sirve', async () => {
    const falso = ctx.anonimo.conSesion('inventada-' + 'x'.repeat(30), 'falso');
    esperar(await falso.get('/api/v1/auth/sesion'), 401);
  });
}
