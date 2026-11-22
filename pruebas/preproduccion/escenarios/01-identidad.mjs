// Administradores, recuperación y activación (RF-AUT-02 a 05, RF-CFG-06). Las invitaciones van
// a example.com: no le llegan a nadie.
import { assert, esperar, esperarError, omitir } from '../lib/registro.mjs';

export const nombre = '01 · Identidad: administradores, recuperación y enlaces';

export default async function (ctx, t) {
  const admin = ctx.admin;
  const correo2 = ctx.d.correo('admin');

  await t.paso('Listar administradores', async () => {
    const r = esperar(await admin.get('/api/v1/administradores'), 200);
    const lista = Array.isArray(r) ? r : r.datos;
    assert.ok(lista.some(x => x.correo === ctx.credenciales.correo), 'la administradora no aparece');
  });

  const admin2 = await t.paso('Invitar a otra administradora', async () => {
    const r = esperar(await admin.post('/api/v1/administradores', { correo: correo2, nombre: 'Camila Prueba' }), 201);
    ctx.creado.admins.push(r.id ?? r.usuario_id ?? r.usuario?.id);
    return r;
  });

  await t.paso('El mismo correo con otras mayúsculas → 409', async () => {
    esperar(await admin.post('/api/v1/administradores', { correo: correo2.toUpperCase(), nombre: 'Otra Camila' }), 409);
  }, { requiere: [admin2] });

  await t.paso('Datos inválidos → 422 con los campos', async () => {
    const r = esperar(await admin.post('/api/v1/administradores', { correo: 'no-es-correo', nombre: 'X' }), 422);
    assert.ok(r.errores.some(e => e.campo === 'correo'));
  });

  await t.paso('Desactivar a la administradora invitada', async () => {
    const id = ctx.creado.admins.at(-1);
    esperar(await admin.patch(`/api/v1/administradores/${id}`, { activo: false }), [200, 204]);
  }, { requiere: [ctx.creado.admins.at(-1)] });

  await t.paso('Administrador inexistente → 404', async () => {
    esperar(await admin.patch('/api/v1/administradores/999999999', { activo: false }), 404);
  });

  await t.paso('Recuperación: misma respuesta exista o no la cuenta (202)', async () => {
    const r = await ctx.anonimo.post('/api/v1/auth/recuperacion', { correo: `nadie.${ctx.d.corrida}@example.com` }, { reintentar429: false });
    if (r.estado === 429) omitir('límite de 10 solicitudes de recuperación por IP y hora');
    esperar(r, 202);
    esperar(await ctx.anonimo.post('/api/v1/auth/recuperacion', { correo: 'no-es-correo' }, { reintentar429: false }), [202, 422, 429]);
  });

  await t.paso('Enlaces inválidos o vencidos se rechazan', async () => {
    const r1 = await ctx.anonimo.post('/api/v1/auth/recuperacion/confirmar', { token: 'token-que-no-existe', contrasena: 'una frase bien larga' });
    assert.ok([400, 404, 410, 422].includes(r1.estado), `recuperación con token falso: ${r1.estado}`);
    const r2 = await ctx.anonimo.post('/api/v1/auth/activacion', { token: 'token-que-no-existe', contrasena: 'una frase bien larga' });
    assert.ok([400, 404, 410, 422].includes(r2.estado), `activación con token falso: ${r2.estado}`);
  });

  await t.paso('Contraseña corta en el cambio de contraseña → 422 (sin cambiarla)', async () => {
    const r = await admin.put('/api/v1/auth/contrasena', { actual: ctx.credenciales.contrasena, nueva: 'corta' });
    esperarError(r, 422);
  });

  await t.paso('Contraseña actual equivocada → rechazada', async () => {
    const r = await admin.put('/api/v1/auth/contrasena', { actual: 'no es la contraseña', nueva: 'una frase nueva bien larga' });
    assert.ok([401, 403, 422].includes(r.estado), `respondió ${r.estado}`);
  });
}
