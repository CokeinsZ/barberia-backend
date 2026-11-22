// Prepara el ambiente de rendimiento por la API: activa la administradora (invitada antes con
// `diamante crear-administrador`; el token en TOKEN o leído de Mailpit), crea tres servicios y seis barberos
// con horario de lunes a sábado, de 9:00 a 19:00, y guarda la sesión en base.json.
import { writeFileSync } from 'node:fs';
import { pedir, exigir } from './comun.mjs';

const MAILPIT = process.env.MAILPIT_URL ?? 'http://localhost:8025';
const CORREO = 'laura@diamante.co';
const CLAVE = 'frase de rendimiento bien larga';

// El token llega por TOKEN (lo imprime `diamante crear-administrador`) o se espera el correo.
let token = process.env.TOKEN;
for (let intento = 0; !token && intento < 30; intento++) {
  const r = await fetch(`${MAILPIT}/api/v1/message/latest`);
  if (r.ok) token = ((await r.json()).Text ?? '').match(/#token=([A-Za-z0-9_-]+)/)?.[1];
  if (!token) await new Promise(listo => setTimeout(listo, 2000));
}
if (!token) throw new Error('No llegó la invitación a Mailpit');
exigir(await pedir('POST', '/api/v1/auth/activacion', { cuerpo: { token, contrasena: CLAVE } }), 204, 'activación');
const entrada = exigir(await pedir('POST', '/api/v1/auth/sesion', { cuerpo: { correo: CORREO, contrasena: CLAVE } }), 200, 'sesión');
const sesion = entrada.cabeceras.get('set-cookie').match(/__Host-sesion=([^;]+)/)[1];

const servicios = [];
for (const [orden, nombre, precio] of [[1, 'Corte clásico', 35000], [2, 'Corte + barba', 50000], [3, 'Diseño de cejas', 10000]]) {
  const r = exigir(await pedir('POST', '/api/v1/servicios', {
    sesion,
    cuerpo: { categoria_id: 1, nombre, descripcion: 'Servicio de la barbería.', precio, duracion_min: 30, orden },
  }), 201, nombre);
  servicios.push(r.json.id);
}
const nombres = ['Santiago', 'Mateo', 'Daniel', 'Felipe', 'Andrés', 'Julián'];
const barberos = [];
for (const [i, nombre] of nombres.entries()) {
  const r = exigir(await pedir('POST', '/api/v1/barberos', {
    sesion,
    cuerpo: {
      tipo_documento: 'CC', numero_documento: `10203040${60 + i}`, nombres: nombre, apellidos: 'Pérez',
      fecha_nacimiento: '1995-03-02', celular: `31055599${10 + i}`, correo: `${nombre.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')}@diamante.co`,
      especialidad: 'Fade', anios_experiencia: 5, fecha_ingreso: '2024-01-15', tipo_vinculacion: 'prestacion_servicios',
      servicios,
      horario: [1, 2, 3, 4, 5, 6].map(dia => ({ dia_semana: dia, inicio: '09:00', fin: '19:00' })),
      visibilidad: { mostrar_en_landing: true, recibe_citas_en_linea: true, acceso_panel: false },
    },
  }), 201, nombre);
  barberos.push(r.json.id);
}
writeFileSync(new URL('./base.json', import.meta.url), JSON.stringify({ sesion, servicios, barberos }, null, 2));
console.log(`Listo: ${servicios.length} servicios y ${barberos.length} barberos.`);
