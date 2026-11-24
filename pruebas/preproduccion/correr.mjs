// Pruebas E2E contra un backend desplegado (preproducción). Ver README.md.
//
//   DIAMANTE_URL=https://back-diamante.ejemplo.co DIAMANTE_SSH=debian@servidor node correr.mjs
//   ESCENARIOS=07,12 node correr.mjs        (solo esos, más 00, 02, 03 y 99, de los que dependen)
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, chmodSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { Cliente } from './lib/cliente.mjs';
import { Registro } from './lib/registro.mjs';
import * as servidor from './lib/servidor.mjs';
import * as datos from './lib/datos.mjs';

const aqui = p => fileURLToPath(new URL(p, import.meta.url));
const URL_BASE = process.env.DIAMANTE_URL;
if (!URL_BASE) {
  console.error('Defina DIAMANTE_URL, p. ej. https://back-diamante.mania-agent.online');
  process.exit(2);
}

// Credenciales de la administradora: .credenciales.json (fuera de git). Se crean activando la
// cuenta con DIAMANTE_TOKEN_ACTIVACION (el token del enlace de `diamante crear-administrador`) y
// DIAMANTE_CORREO_ADMIN; si se pasa un token, reemplaza las guardadas (p. ej., tras reiniciar la base).
const rutaCredenciales = aqui('./.credenciales.json');
let credenciales = existsSync(rutaCredenciales) ? JSON.parse(readFileSync(rutaCredenciales, 'utf8')) : null;
if (process.env.DIAMANTE_TOKEN_ACTIVACION) {
  const correo = process.env.DIAMANTE_CORREO_ADMIN;
  if (!correo) {
    console.error('Con DIAMANTE_TOKEN_ACTIVACION defina también DIAMANTE_CORREO_ADMIN');
    process.exit(2);
  }
  credenciales = { correo, contrasena: randomBytes(24).toString('base64url'), pendiente: process.env.DIAMANTE_TOKEN_ACTIVACION };
}
if (!credenciales) {
  console.error('Faltan las credenciales: cree .credenciales.json ({ "correo", "contrasena" }) o use DIAMANTE_TOKEN_ACTIVACION');
  process.exit(2);
}

const ctx = {
  url: URL_BASE.replace(/\/$/, ''),
  anonimo: new Cliente(URL_BASE),
  admin: null,
  barbero: null,
  credenciales,
  guardarCredenciales() {
    const { pendiente, ...guardar } = this.credenciales;
    writeFileSync(rutaCredenciales, JSON.stringify(guardar, null, 2));
    chmodSync(rutaCredenciales, 0o600);
  },
  servidor,
  d: datos,
  creado: { servicios: {}, barberos: {}, clientes: {}, citas: {}, ventas: {}, bloqueos: [], admins: [] },
  metricas: [],
};

const filtro = (process.env.ESCENARIOS ?? '').split(',').filter(Boolean);
const archivos = readdirSync(aqui('./escenarios'))
  .filter(a => a.endsWith('.mjs'))
  .sort()
  // 00 (sesión), 02 y 03 (servicios y barberos de la corrida) y 99 (limpieza) siempre corren:
  // los demás dependen de ellos.
  .filter(a => ['00', '02', '03', '99'].some(p => a.startsWith(p)) || !filtro.length || filtro.some(f => a.startsWith(f)));

const registro = new Registro();
console.log(`Pruebas E2E de preproducción · ${ctx.url} · corrida ${datos.corrida} · SSH ${servidor.disponible ? process.env.DIAMANTE_SSH : 'no'}`);
const inicio = Date.now();
for (const archivo of archivos) {
  const escenario = await import(aqui(`./escenarios/${archivo}`));
  registro.inicio(escenario.nombre ?? archivo);
  try {
    await escenario.default(ctx, registro);
  } catch (error) {
    registro.resultados.push({ escenario: registro.escenario, paso: '(escenario)', resultado: 'falló', motivo: error.message });
    console.log(`  ✘ el escenario se detuvo: ${error.message}`);
  }
  if (ctx.detener) {
    console.error(`\n✘ Corrida detenida antes de crear datos: ${ctx.detener}`);
    process.exit(2);
  }
}

const r = registro.resumen();
const segundos = Math.round((Date.now() - inicio) / 1000);
console.log(`\n${r.fallaron ? '✘' : '✔'} ${r.pasaron} pasaron · ${r.fallaron} fallaron · ${r.omitidos} omitidos (${r.total} pasos en ${segundos} s)`);
if (ctx.metricas.length) {
  console.log('\nLatencias (vistas desde aquí, incluyen la red):');
  for (const m of ctx.metricas) console.log(`  ${m.nombre}: ${m.n} peticiones · p50 ${m.p50} ms · p95 ${m.p95} ms · máx ${m.max} ms`);
}
for (const f of registro.resultados.filter(x => x.resultado === 'falló')) console.log(`  ✘ [${f.escenario}] ${f.paso}`);
mkdirSync(aqui('./resultados'), { recursive: true });
const informe = aqui(`./resultados/${datos.corrida}.json`);
writeFileSync(informe, JSON.stringify({ url: ctx.url, corrida: datos.corrida, segundos, resumen: r, metricas: ctx.metricas, pasos: registro.resultados }, null, 2));
console.log(`Informe: ${informe}`);
process.exit(r.fallaron ? 1 : 0);
