// Acceso de solo lectura al servidor por SSH (opcional): en preproducción no existen las rutas
// de pruebas, así que el código de verificación de WhatsApp se lee de la cola de mensajes (sin
// credenciales de Meta, los mensajes quedan pendientes con sus variables) y los enlaces de
// acceso de los barberos se piden con `diamante enlace-acceso`. Sin DIAMANTE_SSH, los pasos que
// lo necesitan se omiten.
import { spawn } from 'node:child_process';
import { Omitido } from './registro.mjs';

const SSH = process.env.DIAMANTE_SSH ?? '';
let app = process.env.DIAMANTE_APP ?? '';

export const disponible = Boolean(SSH);

function ssh(argumentos, entrada) {
  return new Promise((resolver, rechazar) => {
    const proceso = spawn('ssh', ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', SSH, ...argumentos]);
    let salida = '';
    let errores = '';
    proceso.stdout.on('data', d => { salida += d; });
    proceso.stderr.on('data', d => { errores += d; });
    proceso.on('close', codigo => (codigo === 0 ? resolver(salida) : rechazar(new Error(`ssh (${codigo}): ${errores || salida}`))));
    if (entrada) proceso.stdin.write(entrada);
    proceso.stdin.end();
  });
}

/// Nombre del proyecto de Docker Compose (en Dokploy, el de la aplicación).
export async function proyecto() {
  if (app) return app;
  const lineas = await ssh(['docker', 'ps', '--filter', 'label=com.docker.compose.service=api', '--format', "'{{.Image}} {{.Label \"com.docker.compose.project\"}}'"]);
  const linea = lineas.split('\n').find(l => l.startsWith('diamante-backend'));
  if (!linea) throw new Error('No encontré el contenedor de la API en el servidor');
  app = linea.split(' ')[1].trim();
  return app;
}

/// Consulta de solo lectura en la base (filas separadas por salto de línea, columnas por `|`).
export async function consultar(sql) {
  if (!/^\s*select\b/i.test(sql)) throw new Error('Solo se permiten consultas SELECT');
  const p = await proyecto();
  const salida = await ssh(['docker', 'exec', '-i', `${p}-db-1`, 'psql', '-U', 'diamante_propietario', '-d', 'diamante', '-At', '-v', 'ON_ERROR_STOP=1'], sql);
  return salida.split('\n').filter(Boolean).map(l => l.split('|'));
}

/// Código de verificación más reciente enviado por WhatsApp a ese celular.
export async function codigoDeVerificacion(celular) {
  if (!/^\d{10}$/.test(celular)) throw new Error('celular inválido');
  for (let intento = 0; intento < 5; intento++) {
    const filas = await consultar(
      `SELECT variables->>'codigo' FROM notificaciones.mensaje WHERE canal = 'whatsapp' AND destinatario = '${celular}'
       AND plantilla = 'codigo_verificacion' ORDER BY id DESC LIMIT 1;`,
    );
    if (filas[0]?.[0]) return filas[0][0];
    await new Promise(listo => setTimeout(listo, 1000));
  }
  throw new Error(`No hay código de verificación para ${celular} en la cola`);
}

/// `diamante enlace-acceso <correo>` dentro de la API: devuelve el token del enlace impreso.
export async function enlaceDeAcceso(correo) {
  if (!/^[^\s'"]+@[^\s'"]+$/.test(correo)) throw new Error('correo inválido');
  const p = await proyecto();
  const salida = await ssh(['docker', 'exec', `${p}-api-1`, '/app/diamante', 'enlace-acceso', correo]).catch(e => e.message);
  const token = salida.match(/#token=([A-Za-z0-9_-]+)/)?.[1];
  if (!token) {
    if (/Uso: diamante/.test(salida)) throw new Omitido('el servidor aún no tiene `diamante enlace-acceso`: despliegue la versión más reciente');
    throw new Error(`enlace-acceso: ${salida.slice(0, 300)}`);
  }
  return token;
}
