// Cuenta del barbero (RF-PBA): activación con un enlace de `diamante enlace-acceso` (por SSH;
// las invitaciones van a example.com), su panel, su horario y sus bloqueos, y cambio de contraseña.
import { randomBytes } from 'node:crypto';
import { cookieDeSesion } from '../lib/cliente.mjs';
import { assert, esperar, omitir } from '../lib/registro.mjs';

export const nombre = '04 · Cuenta y panel del barbero';

export default async function (ctx, t) {
  const b = ctx.creado.barberos;
  const correo = ctx.datosBarbero?.b1?.correo;
  const clave = randomBytes(18).toString('base64url');

  const token = await t.paso('Enlace de activación con `diamante enlace-acceso` (SSH)', async () => {
    if (!ctx.servidor.disponible) omitir('sin DIAMANTE_SSH');
    return ctx.servidor.enlaceDeAcceso(correo);
  }, { requiere: [correo] });

  await t.paso('El barbero activa su cuenta e inicia sesión', async () => {
    esperar(await ctx.anonimo.post('/api/v1/auth/activacion', { token, contrasena: clave }), 204);
    esperar(await ctx.anonimo.post('/api/v1/auth/activacion', { token, contrasena: clave }), 410);
    const r = await ctx.anonimo.post('/api/v1/auth/sesion', { correo, contrasena: clave });
    const s = esperar(r, 200);
    assert.equal(s.panel, '/barbero');
    ctx.barbero = ctx.anonimo.conSesion(cookieDeSesion(r), 'barbero');
    ctx.claveBarbero = clave;
  }, { requiere: [token] });
  const barbero = ctx.barbero;

  await t.paso('«Mi perfil» (/yo)', async () => {
    const yo = esperar(await barbero.get('/api/v1/yo'), 200);
    assert.equal(yo.id ?? yo.barbero?.id, b.b1);
  }, { requiere: [barbero] });

  await t.paso('Su horario y sus servicios (y el de otro barbero no)', async () => {
    esperar(await barbero.get(`/api/v1/barberos/${b.b1}/horario`), 200);
    esperar(await barbero.put(`/api/v1/barberos/${b.b1}/horario`, { franjas: ctx.d.horarioCompleto }), 200);
    esperar(await barbero.get(`/api/v1/barberos/${b.b1}/servicios`), 200);
    esperar(await barbero.get(`/api/v1/barberos/${b.b2}/horario`), [403, 404]);
    esperar(await barbero.put(`/api/v1/barberos/${b.b2}/horario`, { franjas: ctx.d.horarioCompleto }), [403, 404]);
  }, { requiere: [barbero] });

  await t.paso('Sus bloqueos: crear y quitar (y en otro barbero no)', async () => {
    const r = esperar(await barbero.post(`/api/v1/barberos/${b.b1}/bloqueos`, { fecha: ctx.d.fecha(14), desde: '10:00', hasta: '11:00', motivo: 'otro' }), 201);
    esperar(await barbero.delete(`/api/v1/barberos/${b.b1}/bloqueos/${r.id}`), 204);
    esperar(await barbero.post(`/api/v1/barberos/${b.b2}/bloqueos`, { fecha: ctx.d.fecha(14), desde: '10:00', hasta: '11:00', motivo: 'otro' }), [403, 404]);
  }, { requiere: [barbero] });

  await t.paso('Cambia su contraseña: la sesión actual sigue y la anterior contraseña ya no sirve', async () => {
    const nueva = randomBytes(18).toString('base64url');
    esperar(await barbero.put('/api/v1/auth/contrasena', { actual: clave, nueva }), 204);
    esperar(await barbero.get('/api/v1/yo'), 200);
    esperar(await ctx.anonimo.post('/api/v1/auth/sesion', { correo, contrasena: clave }), 401);
    ctx.claveBarbero = nueva;
  }, { requiere: [barbero] });
}
