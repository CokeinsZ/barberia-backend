// Salud, cabeceras de seguridad y sesión de la administradora (RF-AUT-01, ASVS V3 y V14).
import { cookieDeSesion } from '../lib/cliente.mjs';
import { assert, esperar, esperarError } from '../lib/registro.mjs';

export const nombre = '00 · Salud, cabeceras y sesión de la administradora';

export default async function (ctx, t) {
  const a = ctx.anonimo;

  await t.paso('El proceso está vivo y la base responde', async () => {
    assert.equal(esperar(await a.get('/api/salud'), 200).estado, 'viva');
    assert.equal(esperar(await a.get('/api/salud/lista'), 200).estado, 'lista');
  });

  await t.paso('Cabeceras de seguridad en la API (HSTS, CSP, nosniff, DENY, id de solicitud)', async () => {
    const r = await a.get('/api/salud');
    const h = n => r.cabeceras.get(n);
    assert.equal(h('strict-transport-security'), 'max-age=31536000');
    assert.equal(h('content-security-policy'), "default-src 'none'; frame-ancestors 'none'");
    assert.equal(h('x-content-type-options'), 'nosniff');
    assert.equal(h('x-frame-options'), 'DENY');
    assert.equal(h('cache-control'), 'no-store');
    assert.match(h('x-request-id') ?? '', /^[0-9a-f-]{36}$/);
  });

  await t.paso('HTTP redirige a HTTPS', async () => {
    const r = await fetch(ctx.url.replace(/^https:/, 'http:') + '/api/salud', { redirect: 'manual' });
    assert.ok([301, 302, 307, 308].includes(r.status), `HTTP respondió ${r.status}`);
    assert.match(r.headers.get('location') ?? '', /^https:/);
  });

  await t.paso('Rutas inexistentes: 404 en formato Problem Details', async () => {
    const r = await a.get('/api/v1/no-existe');
    esperarError(r, 404, 'no_encontrado');
    assert.equal(r.cabeceras.get('content-type'), 'application/problem+json');
  });

  await t.paso('Una petición que modifica datos desde otro origen se rechaza (CSRF)', async () => {
    const r = await a.post('/api/v1/auth/sesion', { correo: 'x@example.com', contrasena: 'x' }, { origen: 'https://sitio-malicioso.example' });
    esperarError(r, 403, 'origen_invalido');
  });

  if (ctx.credenciales.pendiente) {
    await t.paso('Activar la cuenta de la administradora con el enlace de la invitación', async () => {
      const r = await a.post('/api/v1/auth/activacion', { token: ctx.credenciales.pendiente, contrasena: ctx.credenciales.contrasena });
      esperar(r, 204, 'activación');
      ctx.guardarCredenciales();
      delete ctx.credenciales.pendiente;
    });
  }

  // Límite de 10 inicios de sesión por IP cada 15 minutos (cada corrida usa 2, y 4 con el barbero).
  const limiteDeSesion = r => {
    if (r.estado === 429) throw new Error(`límite de 10 inicios de sesión por IP cada 15 minutos: espere ${r.cabeceras.get('retry-after')} s y repita`);
  };

  await t.paso('Credenciales equivocadas: mensaje genérico', async () => {
    const r = await a.post('/api/v1/auth/sesion', { correo: `nadie.${ctx.d.corrida}@example.com`, contrasena: 'frase equivocada larga' }, { reintentar429: false });
    limiteDeSesion(r);
    esperarError(r, 401, 'credenciales_invalidas');
  });

  const sesion = await t.paso('La administradora inicia sesión (cookie __Host-, Secure, HttpOnly, SameSite=Strict)', async () => {
    const r = await a.post('/api/v1/auth/sesion', { correo: ctx.credenciales.correo, contrasena: ctx.credenciales.contrasena }, { reintentar429: false });
    limiteDeSesion(r);
    esperar(r, 200, 'inicio de sesión');
    const cookie = r.cabeceras.get('set-cookie') ?? '';
    for (const atributo of ['Secure', 'HttpOnly', 'SameSite=Strict', 'Path=/']) assert.ok(cookie.includes(atributo), `falta ${atributo} en la cookie`);
    assert.equal(r.json.panel, '/admin');
    return cookieDeSesion(r);
  });
  if (!sesion) throw new Error('Sin sesión de administradora no se puede seguir');
  ctx.admin = a.conSesion(sesion, 'administradora');

  await t.paso('Consultar la sesión actual', async () => {
    const s = esperar(await ctx.admin.get('/api/v1/auth/sesion'), 200);
    assert.equal(s.usuario.correo, ctx.credenciales.correo);
    ctx.adminId = s.usuario.id;
  });

  await t.paso('Sin sesión, el panel responde 401', async () => {
    esperarError(await a.get('/api/v1/auth/sesion'), 401, 'no_autenticado');
  });
}
